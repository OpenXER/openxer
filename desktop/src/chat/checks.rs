//! Check Setup: plain checks of what each Chat route needs on this PC, run
//! one after another on a worker thread (never on a frame) and listed as
//! rows with a pass or fail mark, one sentence and, where there is one,
//! the fix. No model is asked anything. Every check reuses the code the
//! routes run: `agent_host::detect` (installed, version, signed in, the
//! personal instructions file), `compat` (the tested versions),
//! `openxer_chat::Auth` (signed in, renewable, plan usage),
//! `openxer_chat::http::reach` and `proxy_configured`, and connect's
//! `openxer-cli` path.
use openxer_automation::agent_host::{self as host, compat, Agent, Spawner};
use openxer_automation::connect::Env;
use openxer_chat as oc;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

/// A row's mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Pass,
    Fail,
    /// Worth knowing, not a failure.
    Note,
}

impl Mark {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Mark::Pass => "Pass",
            Mark::Fail => "Fail",
            Mark::Note => "Note",
        }
    }
}

/// The action that fixes a row: the setup list's own actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fix {
    /// Sign In... for this agent (ChatGPT asks its consent first).
    SignIn(String),
    /// Get Claude Code..., Get Codex...: the vendor's install page.
    Install { label: String, url: String },
    /// Update: the vendor's page.
    Update(String),
    /// Look for this agent again.
    CheckAgain(String),
    /// Set Up... the API key or local model.
    SetUp,
}

impl Fix {
    pub(crate) fn label(&self) -> String {
        match self {
            Fix::SignIn(_) => "Sign &In...".into(),
            Fix::Install { label, .. } => label.clone(),
            Fix::Update(_) => "&Update...".into(),
            Fix::CheckAgain(_) => "Check &Again".into(),
            Fix::SetUp => "Set &Up...".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub mark: Mark,
    /// What the row is about: "Claude Code", "ChatGPT", "This PC".
    pub area: String,
    pub text: String,
    pub fix: Option<Fix>,
}

/// What the checks read.
pub(crate) struct Inputs {
    pub env: Env,
    pub spawner: Arc<dyn Spawner>,
    pub auth: Arc<oc::Auth>,
    /// The hosts the routes reach: the ChatGPT route's API and sign-in.
    pub hosts: Vec<oc::http::Url>,
    pub http: oc::http::HttpConfig,
    /// The API key or local model connection, and where its key is.
    pub endpoint: Option<oc::compat::Connection>,
    pub keys: Arc<dyn oc::CredentialStore>,
}

/// The checks of one run, as they come in.
pub(crate) struct Checks {
    pub rows: Vec<Row>,
    /// The checks not done yet (for the progress line).
    pub total: usize,
    receive: Option<Receiver<Row>>,
    cancel: oc::CancelToken,
    pub cancelled: bool,
    /// The profile folder Copy Results hides.
    profile: Option<std::path::PathBuf>,
}

/// How many rows a full run gives (the progress line's "of N"): ChatGPT
/// 3, Claude Code 3, Codex 4, this PC 2 and one per host.
fn total(hosts: usize, endpoint: bool) -> usize {
    3 + if endpoint { 3 } else { 1 } + 3 + 4 + 2 + hosts
}

/// The vendor page of a local agent.
fn page(agent: Agent) -> &'static str {
    match agent {
        Agent::ClaudeCode => super::live::CLAUDE_CODE_URL,
        Agent::Codex => super::live::CODEX_URL,
    }
}

fn id(agent: Agent) -> &'static str {
    match agent {
        Agent::ClaudeCode => super::live::CLAUDE_CODE,
        Agent::Codex => super::live::CODEX,
    }
}

fn row(mark: Mark, area: &str, text: impl Into<String>, fix: Option<Fix>) -> Row {
    Row {
        mark,
        area: area.into(),
        text: text.into(),
        fix,
    }
}

/// The rows of one local agent.
fn agent_rows(agent: Agent, env: &Env, spawner: &dyn Spawner) -> Vec<Row> {
    let name = agent.name();
    let s = host::detect(agent, env, spawner);
    let mut out = vec![];
    let Some(exe) = s.executable.as_ref().filter(|_| s.installed) else {
        out.push(row(
            Mark::Fail,
            name,
            format!("Not installed: {name} was not found on this PC."),
            Some(Fix::Install {
                label: format!("Get {name}..."),
                url: page(agent).into(),
            }),
        ));
        return out;
    };
    out.push(row(
        Mark::Pass,
        name,
        format!("Installed at {}.", exe.display()),
        None,
    ));
    let tested = compat::tested_version(agent);
    out.push(match s.version.as_deref().and_then(compat::short_version) {
        Some(v) if compat::version_notice(agent, &v).is_none() => row(
            Mark::Pass,
            name,
            format!("Version {v}, at or above the tested {tested}."),
            None,
        ),
        Some(v) => row(
            Mark::Fail,
            name,
            format!("Version {v} is older than the tested {tested}."),
            Some(Fix::Update(page(agent).into())),
        ),
        None => row(
            Mark::Fail,
            name,
            "It did not say which version it is.",
            Some(Fix::CheckAgain(id(agent).into())),
        ),
    });
    out.push(match s.signed_in {
        Some(true) => row(Mark::Pass, name, "Signed in.", None),
        Some(false) => row(
            Mark::Fail,
            name,
            "Not signed in.",
            Some(Fix::SignIn(id(agent).into())),
        ),
        None => row(
            Mark::Note,
            name,
            "It did not say whether it is signed in.",
            None,
        ),
    });
    if agent == Agent::Codex {
        out.push(match &s.user_instructions {
            Some(file) => row(
                Mark::Note,
                name,
                format!(
                    "Your personal instructions file {} is read in every chat.",
                    file.display()
                ),
                None,
            ),
            None => row(
                Mark::Pass,
                name,
                "No personal instructions file will be read.",
                None,
            ),
        });
    }
    out
}

/// The rows of the ChatGPT route.
fn plan_rows(auth: &oc::Auth, cancel: &oc::CancelToken) -> Vec<Row> {
    const NAME: &str = "ChatGPT";
    let sign_in = || Some(Fix::SignIn(super::live::CHATGPT.into()));
    match auth.status() {
        oc::SignInStatus::SignedOut => vec![row(Mark::Fail, NAME, "Not signed in.", sign_in())],
        oc::SignInStatus::Expired { .. } => vec![row(
            Mark::Fail,
            NAME,
            "The sign-in ran out after 30 days.",
            sign_in(),
        )],
        oc::SignInStatus::SignedIn { plan_usage, .. } => {
            let mut out = vec![row(Mark::Pass, NAME, "Signed in.", None)];
            out.push(match auth.refresh_now(cancel) {
                Ok(()) => row(Mark::Pass, NAME, "The sign-in can be renewed.", None),
                Err(e) => row(
                    Mark::Fail,
                    NAME,
                    format!("The sign-in could not be renewed: {e}"),
                    sign_in(),
                ),
            });
            out.push(if plan_usage {
                row(
                    Mark::Pass,
                    NAME,
                    "Use of your ChatGPT plan is allowed.",
                    None,
                )
            } else {
                row(
                    Mark::Fail,
                    NAME,
                    "Use of your ChatGPT plan was not allowed at sign-in.",
                    sign_in(),
                )
            });
            out
        }
    }
}

/// The rows about this PC: the hosts, a proxy, `openxer-cli`.
fn pc_rows(inputs: &Inputs, cancel: &oc::CancelToken) -> Vec<Row> {
    const NAME: &str = "This PC";
    let mut out = vec![];
    let mut seen = vec![];
    for url in &inputs.hosts {
        let host = url.host.clone();
        if seen.contains(&host) {
            continue;
        }
        seen.push(host.clone());
        out.push(match oc::http::reach(url, &inputs.http, cancel) {
            Ok(()) => row(Mark::Pass, NAME, format!("Reaches {host}."), None),
            Err(oc::http::HttpError::Proxy) => row(
                Mark::Fail,
                NAME,
                format!("Cannot reach {host}: the connection goes through a proxy server, which the ChatGPT route does not support yet."),
                None,
            ),
            Err(e) => row(Mark::Fail, NAME, format!("Cannot reach {host}: {e}."), None),
        });
    }
    out.push(if oc::http::proxy_configured() {
        row(
            Mark::Note,
            NAME,
            "A proxy server is set up. The ChatGPT route does not support proxies yet.",
            None,
        )
    } else {
        row(Mark::Pass, NAME, "No proxy server is set up.", None)
    });
    out.push(match inputs.env.cli() {
        Err(e) => row(Mark::Fail, NAME, e.message, None),
        Ok(cli) => match host::version(inputs.spawner.as_ref(), cli) {
            Some(v) => row(
                Mark::Pass,
                NAME,
                format!("openxer-cli answers ({v})."),
                None,
            ),
            None => row(
                Mark::Fail,
                NAME,
                format!("openxer-cli at {} did not answer --version.", cli.display()),
                None,
            ),
        },
    });
    out
}

impl Checks {
    /// Starts every check on a worker thread.
    pub(crate) fn start(inputs: Inputs) -> Checks {
        let (send, receive) = mpsc::channel();
        let cancel = oc::CancelToken::new();
        let profile = inputs.env.home.clone();
        let total = {
            let mut hosts: Vec<&str> = inputs.hosts.iter().map(|u| u.host.as_str()).collect();
            hosts.dedup();
            total(hosts.len(), inputs.endpoint.is_some())
        };
        let c = cancel.clone();
        std::thread::spawn(move || {
            let steps: Vec<Box<dyn Fn() -> Vec<Row>>> = vec![
                Box::new(|| plan_rows(&inputs.auth, &c)),
                Box::new(|| {
                    super::live::endpoint::check_rows(inputs.endpoint.as_ref(), &inputs.keys, &c)
                }),
                Box::new(|| agent_rows(Agent::ClaudeCode, &inputs.env, inputs.spawner.as_ref())),
                Box::new(|| agent_rows(Agent::Codex, &inputs.env, inputs.spawner.as_ref())),
                Box::new(|| pc_rows(&inputs, &c)),
            ];
            for step in steps {
                if c.is_cancelled() {
                    return;
                }
                for r in step() {
                    if c.is_cancelled() || send.send(r).is_err() {
                        return;
                    }
                }
            }
        });
        Checks {
            rows: vec![],
            total,
            receive: Some(receive),
            cancel,
            cancelled: false,
            profile,
        }
    }

    /// Takes the rows that came in; never waits.
    pub(crate) fn poll(&mut self) {
        let Some(receive) = &self.receive else {
            return;
        };
        loop {
            match receive.try_recv() {
                Ok(r) => self.rows.push(r),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.receive = None;
                    self.total = self.rows.len();
                    return;
                }
            }
        }
    }

    /// A finished run of these rows (the scripted backend's).
    #[cfg(test)]
    pub(crate) fn finished(rows: Vec<Row>) -> Checks {
        Checks {
            total: rows.len(),
            rows,
            receive: None,
            cancel: oc::CancelToken::new(),
            cancelled: false,
            profile: None,
        }
    }

    /// A mixed run, as a PC with Claude Code and an old Codex shows it.
    #[cfg(test)]
    pub(crate) fn sample() -> Checks {
        let r = |mark, area: &str, text: &str, fix| row(mark, area, text, fix);
        Checks::finished(vec![
            r(
                Mark::Fail,
                "ChatGPT",
                "Not signed in.",
                Some(Fix::SignIn("chatgpt".into())),
            ),
            r(
                Mark::Pass,
                "Claude Code",
                r"Installed at %USERPROFILE%\AppData\Roaming\npm\claude.cmd.",
                None,
            ),
            r(
                Mark::Pass,
                "Claude Code",
                "Version 2.1.330, at or above the tested 2.1.330.",
                None,
            ),
            r(Mark::Pass, "Claude Code", "Signed in.", None),
            r(
                Mark::Pass,
                "Codex",
                r"Installed at %USERPROFILE%\AppData\Roaming\npm\codex.cmd.",
                None,
            ),
            r(
                Mark::Fail,
                "Codex",
                "Version 0.150.0 is older than the tested 0.166.0.",
                Some(Fix::Update(super::live::CODEX_URL.into())),
            ),
            r(
                Mark::Fail,
                "Codex",
                "Not signed in.",
                Some(Fix::SignIn("codex".into())),
            ),
            r(
                Mark::Note,
                "Codex",
                r"Your personal instructions file %USERPROFILE%\.codex\AGENTS.md is read in every chat.",
                None,
            ),
            r(Mark::Pass, "This PC", "Reaches api.openai.com.", None),
            r(Mark::Pass, "This PC", "Reaches auth.openai.com.", None),
            r(Mark::Pass, "This PC", "Reaches api.anthropic.com.", None),
            r(Mark::Pass, "This PC", "Reaches chatgpt.com.", None),
            r(Mark::Pass, "This PC", "No proxy server is set up.", None),
            r(
                Mark::Pass,
                "This PC",
                "openxer-cli answers (openxer-cli 0.1.0).",
                None,
            ),
        ])
    }

    pub(crate) fn running(&self) -> bool {
        self.receive.is_some()
    }

    /// Stops the run; the rows so far stay.
    pub(crate) fn cancel(&mut self) {
        self.cancel.cancel();
        self.receive = None;
        self.cancelled = true;
    }

    /// The rows as plain text for a bug report, with nothing private: the
    /// profile folder reads `%USERPROFILE%` and anything that looks like an
    /// email address is left out. (No row holds a token or an account.)
    pub(crate) fn text(&self) -> String {
        let mut out = String::from("OpenXER Chat setup check\n");
        for r in &self.rows {
            out.push_str(&format!("{}\t{}\t{}\n", r.mark.label(), r.area, r.text));
        }
        if self.cancelled {
            out.push_str("(cancelled before the end)\n");
        }
        redact(&out, self.profile.as_deref())
    }
}

impl Drop for Checks {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// `text` with the profile folder as `%USERPROFILE%` (in any letter case)
/// and words that look like email addresses as `<email>`.
pub(crate) fn redact(text: &str, profile: Option<&Path>) -> String {
    let mut out = text.to_string();
    let homes: Vec<String> = profile
        .map(|p| p.display().to_string())
        .into_iter()
        .chain(std::env::var("USERPROFILE").ok())
        .filter(|h| h.len() > 3)
        // Written with forward slashes too.
        .flat_map(|h| [h.replace('\\', "/"), h])
        .collect();
    for home in homes {
        // ASCII case only, so byte positions stay the same.
        let lower = out.to_ascii_lowercase();
        let needle = home.to_ascii_lowercase();
        let mut result = String::new();
        let mut at = 0;
        while let Some(i) = lower[at..].find(&needle) {
            result.push_str(&out[at..at + i]);
            result.push_str("%USERPROFILE%");
            at += i + needle.len();
        }
        result.push_str(&out[at..]);
        out = result;
    }
    out.split_inclusive(|c: char| c.is_whitespace())
        .map(|word| {
            let core = word.trim_end();
            let tail = &word[core.len()..];
            let looks = core.contains('@')
                && core
                    .rsplit('@')
                    .next()
                    .is_some_and(|domain| domain.contains('.'));
            if looks {
                format!("<email>{tail}")
            } else {
                word.to_string()
            }
        })
        .collect()
}
