//! What the host knows about the agents' versions and how a session copes
//! with an install it was not tested on, plus changing a session's mode or
//! model between messages.
//!
//! The flags the host passes were checked against [`TESTED`] only. An older
//! version still gets a try (the window shows [`version_notice`]); one that
//! refuses a flag or setting at once is reported by [`launch_problem`] as
//! not supported, with the installed version and the first line of what
//! it said, and a launch that never answers ends after [`READY_TIMEOUT`].
use super::{
    check_model_name, claude, connect, instructions_file, rules, Agent, AgentError, ModelChoice,
    Session,
};
use crate::chat_tools::ChatMode;
use std::path::PathBuf;
use std::time::Duration;

/// The versions the launch flags were checked on (the first line of each
/// program's `--version`).
pub const TESTED: &[(Agent, &str)] = &[(Agent::ClaudeCode, "2.1.330"), (Agent::Codex, "0.166.0")];

/// How long a starting agent may take to say it is ready (its first line
/// of output) before the launch counts as broken.
pub const READY_TIMEOUT: Duration = Duration::from_secs(20);

/// The version `agent` was tested on.
pub fn tested_version(agent: Agent) -> &'static str {
    TESTED
        .iter()
        .find(|(a, _)| *a == agent)
        .map(|(_, v)| *v)
        .unwrap_or("")
}

/// The first `major.minor.patch` in a `--version` line ("2.1.330 (Claude
/// Code)", "codex-cli 0.166.0").
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|w| w.contains('.'))
        .find_map(|w| {
            let mut parts = w.split('.').filter(|p| !p.is_empty());
            let major = parts.next()?.parse().ok()?;
            let minor = parts.next()?.parse().ok()?;
            let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            Some((major, minor, patch))
        })
}

/// The version as the window names it ("2.1.330"), from a `--version` line.
pub fn short_version(text: &str) -> Option<String> {
    parse_version(text).map(|(a, b, c)| format!("{a}.{b}.{c}"))
}

/// One line for an installed version older than the tested one; `None`
/// when it is the same or newer, or cannot be read.
pub fn version_notice(agent: Agent, version: &str) -> Option<String> {
    let found = parse_version(version)?;
    let tested = parse_version(tested_version(agent))?;
    (found < tested).then(|| {
        format!(
            "{} {} is older than the version OpenXER was tested with ({}). It may not work; update {} if it does not.",
            agent.name(),
            short_version(version).unwrap_or_default(),
            tested_version(agent),
            agent.name()
        )
    })
}

/// Words of an agent that refused how it was started: an option, a
/// feature name or a setting it does not know.
const LAUNCH_ERRORS: &[&str] = &[
    "unknown option",
    "unknown argument",
    "unexpected argument",
    "unrecognized option",
    "unrecognized argument",
    "invalid value",
    "unknown feature",
    "unknown configuration",
    "unknown config",
    "unknown field",
    "invalid config",
    "error loading config",
    "failed to parse",
    "error parsing",
    "too many arguments",
];

/// When `output` (what a process that stopped printed) shows the agent
/// refused its launch flags or settings: the first such line.
pub fn launch_problem(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            LAUNCH_ERRORS.iter().any(|k| lower.contains(k))
        })
        .map(|line| super::clip(line, 200))
}

/// The status line for an agent version that refused its launch: the
/// installed version, that it is not supported, what to update, and the
/// first line of its error.
pub fn unsupported_message(agent: Agent, version: Option<&str>, problem: &str) -> String {
    let version = version
        .and_then(short_version)
        .map(|v| format!(" {v}"))
        .unwrap_or_default();
    format!(
        "{}{version} is not supported by this version of OpenXER (tested with {}). Update {}. It said: {problem}",
        agent.name(),
        tested_version(agent),
        agent.name()
    )
}

/// The status line for a launch that did not answer in time.
pub fn no_answer_message(agent: Agent) -> String {
    format!(
        "{} did not start within {} seconds. Check that it runs from a command prompt, or update it.",
        agent.name(),
        READY_TIMEOUT.as_secs()
    )
}

/// The agent's program on `env`'s `PATH`, as a program can be started:
/// an `.exe` (a native or winget install), or the `.cmd` or `.bat` shim an
/// npm install puts beside a PowerShell one, never the `.ps1` itself.
pub fn find_program(agent: Agent, env: &connect::Env) -> Option<PathBuf> {
    let path = env.path.as_ref()?;
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|e| format!("{}.{e}", agent.command()))
            .collect()
    } else {
        vec![agent.command().to_string()]
    };
    std::env::split_paths(path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

impl Session {
    /// Whether the next message goes to a process that is running now
    /// (Claude Code between messages); false when it starts a new one
    /// (always for Codex, and after a stop or a change of mode or model).
    pub fn has_live_process(&self) -> bool {
        self.options.agent == Agent::ClaudeCode && self.process_running()
    }

    /// Change the mode or model for the next message, keeping the
    /// conversation: the config files are written again and a running
    /// Claude Code process is stopped, so the next message resumes the
    /// conversation with the new tools and model (Codex starts a process
    /// per message anyway). Refused while a reply runs.
    pub fn reconfigure(&mut self, mode: ChatMode, model: ModelChoice) -> Result<(), AgentError> {
        if self.options.mode == mode && self.options.model == model {
            return Ok(());
        }
        if let ModelChoice::Named(name) = &model {
            check_model_name(name)?;
        }
        if self.is_busy() {
            return Err(AgentError::Busy);
        }
        self.options.mode = mode;
        self.options.model = model;
        let mut files = vec![(instructions_file(&self.options), rules(&self.options))];
        if self.options.agent == Agent::ClaudeCode {
            files.push((
                claude::config_path(&self.options),
                claude::mcp_config(&self.options).to_string(),
            ));
        }
        for (path, text) in files {
            std::fs::write(&path, text).map_err(|e| AgentError::Failed {
                message: format!("Cannot write {}: {e}", path.display()),
            })?;
        }
        self.stop_process();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_and_compared_with_the_tested_ones() {
        assert_eq!(parse_version("2.1.330 (Claude Code)"), Some((2, 1, 330)));
        assert_eq!(parse_version("codex-cli 0.166.0"), Some((0, 166, 0)));
        assert_eq!(parse_version("no version"), None);
        assert!(version_notice(Agent::ClaudeCode, "2.1.330 (Claude Code)").is_none());
        assert!(version_notice(Agent::ClaudeCode, "2.2.0 (Claude Code)").is_none());
        let old = version_notice(Agent::ClaudeCode, "1.0.88 (Claude Code)").unwrap();
        assert!(old.contains("Claude Code 1.0.88 is older"), "{old}");
        assert!(old.contains("2.1.330"));
        assert!(version_notice(Agent::Codex, "codex-cli 0.150.2").is_some());
    }

    #[test]
    fn a_refused_launch_is_named_by_its_first_line() {
        let out = "Usage: claude [options]\nerror: unknown option '--restricted'\n  at main.js:12";
        assert_eq!(
            launch_problem(out).as_deref(),
            Some("error: unknown option '--restricted'")
        );
        assert!(launch_problem("Not logged in. Please run /login").is_none());
        let line = unsupported_message(
            Agent::Codex,
            Some("codex-cli 0.100.0"),
            "Error: unknown feature `apps`",
        );
        assert_eq!(
            line,
            "Codex 0.100.0 is not supported by this version of OpenXER (tested with 0.166.0). Update Codex. It said: Error: unknown feature `apps`"
        );
    }
}
