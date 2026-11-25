//! Fakes for the live backend's tests: a scripted Claude Code and Codex
//! behind the agent host's `Spawner` (replaying the recorded transcripts in
//! `automation/tests/fixtures/agent/`), fake installs on a temp profile,
//! and a ChatGPT route on the chat crate's fake sign-in service and API
//! (`openxer_chat::testkit`). No real agent runs, no browser opens and no
//! real host or Credential Manager is touched.
use super::{Config, LiveBackend, Opener};
use openxer_automation::agent_host::{Agent, AgentProcess, Launch, Spawner};
use openxer_automation::connect::Env;
use openxer_chat as oc;
use openxer_chat::testkit::{self as kit, FakeServer, Reply};
use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CLAUDE_FIXTURE: &str =
    include_str!("../../../../automation/tests/fixtures/agent/claude-two-turns.jsonl");
const CODEX_FIXTURE: &str =
    include_str!("../../../../automation/tests/fixtures/agent/codex-two-turns.jsonl");

/// A recorded transcript cut into its turns.
fn turns(text: &str, end: &str) -> Vec<Vec<String>> {
    let mut out = vec![vec![]];
    for line in text.lines() {
        out.last_mut().unwrap().push(line.to_string());
        if line.contains(end) {
            out.push(vec![]);
        }
    }
    out.retain(|t| !t.is_empty());
    out
}

/// Claude Code's two recorded turns: a question about the critical path,
/// then a proposal renaming PM0910.
pub(crate) fn claude_turns() -> Vec<Vec<String>> {
    turns(CLAUDE_FIXTURE, "\"type\":\"result\"")
}

/// Codex's two recorded turns.
pub(crate) fn codex_turns() -> Vec<Vec<String>> {
    turns(CODEX_FIXTURE, "\"type\":\"turn.completed\"")
}

/// How a fake agent behaves.
#[derive(Clone, Debug)]
pub(crate) struct Behaviour {
    pub version: String,
    pub signed_in: bool,
    /// Every conversation launch fails at once with this error output.
    pub refuses: Option<String>,
    /// Conversation launches print nothing and keep running.
    pub hangs: bool,
    /// A conversation launch takes this long to start (a slow PC).
    pub start_delay: Duration,
    /// The turns still to give, in order.
    pub turns: VecDeque<Vec<String>>,
}

impl Behaviour {
    pub(crate) fn of(agent: Agent) -> Behaviour {
        let (version, turns) = match agent {
            Agent::ClaudeCode => ("2.1.330 (Claude Code)", claude_turns()),
            Agent::Codex => ("codex-cli 0.166.0", codex_turns()),
        };
        Behaviour {
            version: version.into(),
            signed_in: true,
            refuses: None,
            hangs: false,
            start_delay: Duration::ZERO,
            turns: turns.into(),
        }
    }
}

/// What the fake saw.
#[derive(Default)]
pub(crate) struct Seen {
    pub launches: Vec<Launch>,
    /// What each conversation process got on stdin.
    pub stdin: Vec<String>,
}

#[derive(Default)]
pub(crate) struct FakeAgents {
    pub agents: Arc<Mutex<HashMap<Agent, Behaviour>>>,
    pub seen: Arc<Mutex<Seen>>,
    /// Processes still running.
    pub alive: Arc<AtomicUsize>,
    /// Processes stopped through `kill`.
    pub kills: Arc<AtomicUsize>,
    /// A sign-in in progress ends (true: signed in).
    pub sign_in_done: Arc<Mutex<Option<bool>>>,
}

impl FakeAgents {
    pub(crate) fn new() -> Arc<FakeAgents> {
        Arc::new(FakeAgents::default())
    }

    pub(crate) fn set(&self, agent: Agent, b: Behaviour) {
        self.agents.lock().unwrap().insert(agent, b);
    }

    pub(crate) fn with(&self, agent: Agent, f: impl FnOnce(&mut Behaviour)) {
        let mut all = self.agents.lock().unwrap();
        f(all.entry(agent).or_insert_with(|| Behaviour::of(agent)));
    }

    pub(crate) fn launches(&self) -> Vec<Launch> {
        self.seen.lock().unwrap().launches.clone()
    }

    pub(crate) fn stdin(&self) -> Vec<String> {
        self.seen.lock().unwrap().stdin.clone()
    }

    pub(crate) fn alive(&self) -> usize {
        self.alive.load(Ordering::SeqCst)
    }
}

fn agent_of(program: &Path) -> Option<Agent> {
    let name = program.file_name()?.to_string_lossy().to_ascii_lowercase();
    if name.starts_with("claude") {
        Some(Agent::ClaudeCode)
    } else if name.starts_with("codex") {
        Some(Agent::Codex)
    } else {
        None
    }
}

enum Plan {
    /// Print this, then exit with the code.
    Print(String, String, i32),
    /// The agent's own login: runs until told it finished.
    SignIn,
    /// A Claude conversation: one turn per stdin line.
    Claude(Agent),
    /// One Codex turn: read stdin to its end, answer, exit.
    Codex(Agent),
    Hang,
}

struct ChannelReader {
    receive: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_empty() {
            match self.receive.recv() {
                Ok(bytes) => self.buffer = bytes,
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buffer.len());
        out[..n].copy_from_slice(&self.buffer[..n]);
        self.buffer.drain(..n);
        Ok(n)
    }
}

struct ChannelWriter {
    send: Sender<Option<String>>,
    seen: Arc<Mutex<Seen>>,
    partial: Vec<u8>,
}

impl Write for ChannelWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.partial.extend_from_slice(bytes);
        while let Some(i) = self.partial.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=i).collect();
            let line = String::from_utf8_lossy(&line).trim_end().to_string();
            self.seen.lock().unwrap().stdin.push(line.clone());
            let _ = self.send.send(Some(line));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for ChannelWriter {
    fn drop(&mut self) {
        if !self.partial.is_empty() {
            let line = String::from_utf8_lossy(&self.partial).to_string();
            self.seen.lock().unwrap().stdin.push(line);
        }
        // Closed: a Codex turn starts.
        let _ = self.send.send(None);
    }
}

struct FakeProcess {
    stdin: Option<Box<dyn Write + Send>>,
    stdout: Option<Box<dyn Read + Send>>,
    stderr: Option<Box<dyn Read + Send>>,
    killed: Arc<AtomicBool>,
    exit: Arc<Mutex<Option<i32>>>,
    kills: Arc<AtomicUsize>,
}

impl AgentProcess for FakeProcess {
    fn take_stdin(&mut self) -> Option<Box<dyn Write + Send>> {
        self.stdin.take()
    }
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.stdout.take()
    }
    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.stderr.take()
    }
    fn kill(&mut self) -> io::Result<()> {
        if !self.killed.swap(true, Ordering::SeqCst) && self.exit.lock().unwrap().is_none() {
            self.kills.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
    fn try_wait(&mut self) -> io::Result<Option<i32>> {
        Ok(*self.exit.lock().unwrap())
    }
}

impl Drop for FakeProcess {
    fn drop(&mut self) {
        self.killed.store(true, Ordering::SeqCst);
    }
}

impl Spawner for FakeAgents {
    fn spawn(&self, launch: &Launch) -> io::Result<Box<dyn AgentProcess>> {
        self.seen.lock().unwrap().launches.push(launch.clone());
        let args: Vec<&str> = launch.args.iter().map(String::as_str).collect();
        let agent = agent_of(&launch.program);
        let behaviour = agent.and_then(|a| self.agents.lock().unwrap().get(&a).cloned());
        let plan = match (agent, &behaviour, args.as_slice()) {
            (None, _, ["--version"]) => Plan::Print("openxer-cli 0.1.0\n".into(), String::new(), 0),
            (Some(_), None, _) => {
                return Err(io::Error::new(io::ErrorKind::NotFound, "not installed"))
            }
            (Some(_), Some(b), ["--version"]) => {
                Plan::Print(format!("{}\n", b.version), String::new(), 0)
            }
            (Some(_), Some(b), ["auth", "status", "--json"]) => Plan::Print(
                format!("{{\"loggedIn\": {}}}\n", b.signed_in),
                String::new(),
                0,
            ),
            (Some(_), Some(b), ["login", "status"]) if b.signed_in => {
                Plan::Print("Logged in using ChatGPT\n".into(), String::new(), 0)
            }
            (Some(_), Some(_), ["login", "status"]) => {
                Plan::Print(String::new(), "Not logged in\n".into(), 1)
            }
            (Some(_), Some(_), ["auth", "login"] | ["login"]) => Plan::SignIn,
            (Some(_), Some(_), ["purge", ..] | ["delete", ..]) => {
                Plan::Print(String::new(), String::new(), 0)
            }
            (Some(_), Some(b), _) if b.refuses.is_some() => {
                Plan::Print(String::new(), b.refuses.clone().unwrap_or_default(), 2)
            }
            (Some(_), Some(b), _) if b.hangs => Plan::Hang,
            (Some(a @ Agent::ClaudeCode), Some(b), _) => {
                std::thread::sleep(b.start_delay);
                Plan::Claude(a)
            }
            (Some(a @ Agent::Codex), Some(b), _) => {
                std::thread::sleep(b.start_delay);
                Plan::Codex(a)
            }
            _ => Plan::Print(String::new(), "unexpected".into(), 1),
        };
        let (out, receive) = mpsc::channel::<Vec<u8>>();
        let (send_in, read_in) = mpsc::channel::<Option<String>>();
        let killed = Arc::new(AtomicBool::new(false));
        let exit = Arc::new(Mutex::new(None));
        let stderr = match &plan {
            Plan::Print(_, e, _) => e.clone().into_bytes(),
            _ => vec![],
        };
        self.alive.fetch_add(1, Ordering::SeqCst);
        let (k, e, alive) = (killed.clone(), exit.clone(), self.alive.clone());
        let done = self.sign_in_done.clone();
        let agents = self.agents.clone();
        std::thread::spawn(move || {
            let next_turn = |a: Agent| -> Vec<String> {
                agents
                    .lock()
                    .unwrap()
                    .get_mut(&a)
                    .and_then(|b| b.turns.pop_front())
                    .unwrap_or_default()
            };
            let mut code = 0;
            match plan {
                Plan::Print(text, _, c) => {
                    let _ = out.send(text.into_bytes());
                    code = c;
                }
                Plan::SignIn => loop {
                    if k.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Some(ok) = *done.lock().unwrap() {
                        code = if ok { 0 } else { 1 };
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                },
                Plan::Hang => {
                    while !k.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                Plan::Claude(a) => 'turns: loop {
                    loop {
                        if k.load(Ordering::SeqCst) {
                            break 'turns;
                        }
                        match read_in.recv_timeout(Duration::from_millis(5)) {
                            Ok(Some(_)) => break,
                            Ok(None) | Err(RecvTimeoutError::Disconnected) => break 'turns,
                            Err(RecvTimeoutError::Timeout) => {}
                        }
                    }
                    for line in next_turn(a) {
                        if k.load(Ordering::SeqCst) {
                            break 'turns;
                        }
                        let _ = out.send(format!("{line}\n").into_bytes());
                    }
                },
                Plan::Codex(a) => {
                    loop {
                        if k.load(Ordering::SeqCst) {
                            break;
                        }
                        match read_in.recv_timeout(Duration::from_millis(5)) {
                            Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
                            _ => {}
                        }
                    }
                    if !k.load(Ordering::SeqCst) {
                        for line in next_turn(a) {
                            let _ = out.send(format!("{line}\n").into_bytes());
                        }
                    }
                }
            }
            *e.lock().unwrap() = Some(if k.load(Ordering::SeqCst) { -1 } else { code });
            alive.fetch_sub(1, Ordering::SeqCst);
            drop(out);
        });
        Ok(Box::new(FakeProcess {
            stdin: launch.stdin.then(|| {
                Box::new(ChannelWriter {
                    send: send_in,
                    seen: self.seen.clone(),
                    partial: vec![],
                }) as Box<dyn Write + Send>
            }),
            stdout: Some(Box::new(ChannelReader {
                receive,
                buffer: vec![],
            })),
            stderr: Some(Box::new(io::Cursor::new(stderr))),
            killed,
            exit,
            kills: self.kills.clone(),
        }))
    }
}

/// A temp profile with the agents installed as `shape` puts them, a dummy
/// `openxer-cli`, and nothing else.
pub(crate) struct Profile {
    pub root: PathBuf,
    pub env: Env,
}

/// How an agent is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// npm: `<name>.cmd` with a `.ps1` beside it in `AppData\Roaming\npm`.
    Npm,
    /// A winget or native install: `<name>.exe`.
    Exe,
}

impl Profile {
    /// A profile under the temp folder, named `tag` (spaces and other
    /// characters as given).
    pub(crate) fn new(tag: &str) -> Profile {
        let root = std::env::temp_dir()
            .join("openxer-chat-tests")
            .join(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let bin = root.join("bin");
        for d in [&home, &bin] {
            std::fs::create_dir_all(d).unwrap();
        }
        let cli = bin.join(openxer_automation::connect::CLI_NAME);
        std::fs::write(&cli, b"").unwrap();
        let env = Env {
            appdata: Some(home.join("AppData").join("Roaming")),
            local_appdata: Some(home.join("AppData").join("Local")),
            home: Some(home.clone()),
            codex_home: None,
            path: Some(bin.clone().into_os_string()),
            cli: Some(cli),
        };
        Profile { root, env }
    }

    /// Installs `agent` as `shape`; the folder joins PATH. Its path.
    pub(crate) fn install(&mut self, agent: Agent, shape: Shape) -> PathBuf {
        let home = self.env.home.clone().unwrap();
        let (dir, files): (PathBuf, Vec<String>) = match shape {
            Shape::Npm => (
                home.join("AppData").join("Roaming").join("npm"),
                if cfg!(windows) {
                    vec![
                        format!("{}.ps1", agent.command()),
                        format!("{}.cmd", agent.command()),
                        agent.command().to_string(),
                    ]
                } else {
                    vec![
                        format!("{}.ps1", agent.command()),
                        agent.command().to_string(),
                    ]
                },
            ),
            Shape::Exe => (
                home.join("AppData")
                    .join("Local")
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links"),
                if cfg!(windows) {
                    vec![format!("{}.exe", agent.command())]
                } else {
                    vec![agent.command().to_string()]
                },
            ),
        };
        std::fs::create_dir_all(&dir).unwrap();
        for f in &files {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let mut paths: Vec<PathBuf> =
            std::env::split_paths(self.env.path.as_ref().unwrap()).collect();
        if !paths.contains(&dir) {
            paths.push(dir.clone());
        }
        self.env.path = Some(std::env::join_paths(paths).unwrap());
        dir.join(if cfg!(windows) {
            match shape {
                Shape::Npm => format!("{}.cmd", agent.command()),
                Shape::Exe => format!("{}.exe", agent.command()),
            }
        } else {
            agent.command().to_string()
        })
    }

    /// Codex's personal instructions file in its home folder.
    pub(crate) fn codex_instructions(&self, text: &str) -> PathBuf {
        let dir = self.env.home.clone().unwrap().join(".codex");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("AGENTS.md");
        std::fs::write(&file, text).unwrap();
        file
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The ChatGPT route on the chat crate's fakes: its sign-in service and
/// browser, and an API whose replies the test queues.
pub(crate) struct Plan2 {
    pub fx: Arc<kit::Fixture>,
    /// Replies of `POST /v1/responses`, in order.
    pub replies: Arc<Mutex<VecDeque<Reply>>>,
}

impl Plan2 {
    pub(crate) fn new() -> Plan2 {
        // A loaded PC can be slow to come back from the fake browser.
        Plan2::with(|c| c.callback_timeout = Duration::from_secs(30))
    }

    /// With the sign-in service's settings changed by `tune`.
    pub(crate) fn with(tune: impl FnOnce(&mut oc::AuthConfig)) -> Plan2 {
        let fx = Arc::new(kit::Fixture::with_config(tune));
        let replies: Arc<Mutex<VecDeque<Reply>>> = Arc::default();
        let r = replies.clone();
        fx.server.route("POST", "/v1/responses", move |_| {
            r.lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| kit::text_reply("(no reply queued)"))
        });
        fx.server.route_json(
            "GET",
            "/v1/models",
            200,
            serde_json::json!({"models": [
                {"slug": "model-a", "display_name": "Model A", "visibility": "list"},
                {"slug": "model-b", "display_name": "Model B", "visibility": "list"}]}),
        );
        Plan2 { fx, replies }
    }

    /// The API config pointing at the fake.
    pub(crate) fn api(&self) -> oc::ApiConfig {
        oc::ApiConfig {
            base_url: self.fx.server.url("/v1"),
            http: oc::http::HttpConfig {
                connect_timeout: Duration::from_secs(2),
                read_timeout: Duration::from_secs(10),
            },
            retry_delays: vec![Duration::from_millis(10)],
            max_retry_after: Duration::from_millis(50),
            ..oc::ApiConfig::openai()
        }
    }

    pub(crate) fn queue(&self, reply: Reply) {
        self.replies.lock().unwrap().push_back(reply);
    }

    pub(crate) fn server(&self) -> &FakeServer {
        &self.fx.server
    }

    pub(crate) fn requests(&self) -> Vec<serde_json::Value> {
        self.fx
            .server
            .seen("POST", "/v1/responses")
            .iter()
            .map(|s| s.json())
            .collect()
    }
}

/// What the opener was asked to open.
pub(crate) fn recording_opener() -> (Opener, Arc<Mutex<Vec<String>>>) {
    let opened: Arc<Mutex<Vec<String>>> = Arc::default();
    let o = opened.clone();
    (
        Arc::new(move |url: &str| {
            o.lock().unwrap().push(url.to_string());
            Ok(())
        }),
        opened,
    )
}

/// Everything a backend test drives.
pub(crate) struct Rig {
    pub agents: Arc<FakeAgents>,
    pub profile: Profile,
    pub plan: Plan2,
    pub opened: Arc<Mutex<Vec<String>>>,
    pub opener: Opener,
    pub temp: PathBuf,
    /// What the fake browser does with a sign-in page.
    pub browser: Arc<Mutex<kit::Browser>>,
    pub ready_timeout: Duration,
    /// The endpoint route's keys.
    pub keys: Arc<oc::MemoryStore>,
}

impl Rig {
    /// Claude Code and Codex installed (npm) and signed in; ChatGPT signed
    /// out.
    pub(crate) fn new(tag: &str) -> Rig {
        let mut rig = Rig::bare(tag);
        for a in [Agent::ClaudeCode, Agent::Codex] {
            rig.profile.install(a, Shape::Npm);
            rig.agents.set(a, Behaviour::of(a));
        }
        rig
    }

    /// Nothing installed, ChatGPT signed out.
    pub(crate) fn bare(tag: &str) -> Rig {
        Rig::bare_with(tag, Plan2::new())
    }

    /// Nothing installed, with this ChatGPT route.
    pub(crate) fn bare_with(tag: &str, plan: Plan2) -> Rig {
        let (record, opened) = recording_opener();
        let profile = Profile::new(tag);
        let temp = profile.root.join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        let browser = Arc::new(Mutex::new(kit::Browser::approve_first()));
        // The opener records every page; a sign-in page also goes to the
        // fake browser, which answers the callback itself.
        let (fx, how) = (plan.fx.clone(), browser.clone());
        let opener: Opener = Arc::new(move |url: &str| {
            record(url)?;
            if url.contains("/authorize?") {
                let how = how.lock().unwrap().clone();
                fx.browser(how)(url)?;
            }
            Ok(())
        });
        Rig {
            agents: FakeAgents::new(),
            profile,
            plan,
            opened,
            opener,
            temp,
            browser,
            ready_timeout: Duration::from_secs(20),
            keys: Arc::new(oc::MemoryStore::new()),
        }
    }

    pub(crate) fn config(&self) -> Config {
        Config {
            env: self.profile.env.clone(),
            spawner: self.agents.clone(),
            auth: self.plan.fx.auth.clone(),
            api: self.plan.api(),
            opener: self.opener.clone(),
            temp: self.temp.clone(),
            hosts: vec![self.plan.server().url("/")],
            ready_timeout: self.ready_timeout,
            instructions: self.instructions(),
            keys: self.keys.clone(),
        }
    }

    /// The user's instructions file of this profile's data folder.
    pub(crate) fn instructions(&self) -> PathBuf {
        openxer_automation::chat_prompt::instructions_path(&self.profile.root.join("data"))
    }

    pub(crate) fn backend(&self) -> LiveBackend {
        LiveBackend::new(self.config())
    }

    /// Signs the ChatGPT route in on the fake service.
    pub(crate) fn sign_in_chatgpt(&self) {
        let b = self.plan.fx.browser(kit::Browser::approve_first());
        self.plan
            .fx
            .auth
            .sign_in(&b, &oc::CancelToken::new())
            .expect("fake sign-in");
    }

    /// The work folders made under the rig's temp folder.
    pub(crate) fn work_dirs(&self) -> Vec<PathBuf> {
        std::fs::read_dir(&self.temp)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(super::work::PREFIX))
            })
            .collect()
    }
}
