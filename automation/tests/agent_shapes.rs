//! Detection through the real spawner on the install shapes a fresh user
//! has: an npm `.cmd` shim (with its `.ps1` and shell script beside it), in
//! a profile folder whose name has spaces and non-ASCII letters. The shims
//! are small batch files written here that answer like the agents do; no
//! real agent is installed or run. Windows only (batch files).
#![cfg(windows)]
use openxer_automation::agent_host::{detect, Agent, SystemSpawner};
use openxer_automation::connect::Env;
use std::path::{Path, PathBuf};

const CLAUDE_CMD: &str = "@echo off\r\nif \"%~1\"==\"--version\" (\r\n  echo 2.1.330 ^(Claude Code^)\r\n  exit /b 0\r\n)\r\nif \"%~1\"==\"auth\" (\r\n  echo {\"loggedIn\": true}\r\n  exit /b 0\r\n)\r\nexit /b 1\r\n";
const CODEX_CMD: &str = "@echo off\r\nif \"%~1\"==\"--version\" (\r\n  echo codex-cli 0.166.0\r\n  exit /b 0\r\n)\r\nif \"%~1\"==\"login\" (\r\n  echo Not logged in\r\n  exit /b 1\r\n)\r\nexit /b 1\r\n";

/// An npm-style install of both agents under `root`.
fn npm_profile(root: &Path) -> (Env, PathBuf, PathBuf) {
    let npm = root
        .join("Ünïcödé Üser (test)")
        .join("AppData")
        .join("Roaming")
        .join("npm");
    std::fs::create_dir_all(&npm).unwrap();
    for (name, body) in [("claude", CLAUDE_CMD), ("codex", CODEX_CMD)] {
        std::fs::write(npm.join(format!("{name}.cmd")), body).unwrap();
        // npm also writes these; neither may be started.
        std::fs::write(npm.join(format!("{name}.ps1")), "exit 9").unwrap();
        std::fs::write(npm.join(name), "#!/bin/sh\nexit 9\n").unwrap();
    }
    let env = Env {
        path: Some(npm.clone().into_os_string()),
        ..Env::default()
    };
    (env, npm.join("claude.cmd"), npm.join("codex.cmd"))
}

#[test]
fn npm_shims_in_a_path_with_spaces_and_non_ascii_letters_are_detected() {
    let dir = tempfile::tempdir().unwrap();
    let (env, claude, codex) = npm_profile(dir.path());
    // cmd.exe starts slowly on a loaded PC: up to three looks, each
    // reported when it falls short.
    let look = |agent: Agent| {
        let mut last = None;
        for _ in 0..3 {
            let s = detect(agent, &env, &SystemSpawner);
            if s.version.is_some() && s.signed_in.is_some() {
                return s;
            }
            eprintln!("detection fell short, looking again: {s:?}");
            last = Some(s);
        }
        last.unwrap()
    };
    let c = look(Agent::ClaudeCode);
    assert!(c.installed, "{c:?}");
    assert_eq!(c.executable.as_deref(), Some(claude.as_path()));
    assert_eq!(c.version.as_deref(), Some("2.1.330 (Claude Code)"), "{c:?}");
    assert_eq!(c.signed_in, Some(true), "{c:?}");
    let x = look(Agent::Codex);
    assert_eq!(x.executable.as_deref(), Some(codex.as_path()));
    assert_eq!(x.version.as_deref(), Some("codex-cli 0.166.0"), "{x:?}");
    assert_eq!(x.signed_in, Some(false), "{x:?}");
}

#[test]
fn a_ps1_alone_is_not_an_install() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("only ps1");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("claude.ps1"), "exit 0").unwrap();
    let env = Env {
        path: Some(bin.into_os_string()),
        ..Env::default()
    };
    let s = detect(Agent::ClaudeCode, &env, &SystemSpawner);
    assert!(
        !s.installed,
        "a PowerShell script cannot be started as a program"
    );
}
