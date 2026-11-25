//! Unit tests of the connect module: pure text functions on strings, and
//! the file layer on temporary folders only (never a real client config).
use super::*;
use std::fs;

fn yes(_: &Path) -> bool {
    true
}
fn no(_: &Path) -> bool {
    false
}

fn cli() -> PathBuf {
    PathBuf::from(r"C:\Program Files\OpenXER\openxer-cli.exe")
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap()
}

// ------------------------------------------------------------------- JSON

#[test]
fn json_connect_into_missing_or_empty_config() {
    for text in ["", "  \n", "{}", "\u{feff}{}"] {
        let out = json_connect(text, &cli()).unwrap();
        assert_eq!(
            parse(&out),
            json!({"mcpServers":{"openxer":{"command":r"C:\Program Files\OpenXER\openxer-cli.exe","args":["mcp"]}}}),
            "{text:?}"
        );
        assert_eq!(json_entry(&out, &cli(), &yes), Entry::Connected);
        // A blank file gets a whole new text; an object keeps its own end.
        assert_eq!(out.ends_with('\n'), text.trim().is_empty(), "{text:?}");
        assert_eq!(out.starts_with('\u{feff}'), text.starts_with('\u{feff}'));
    }
}

/// Every byte outside the `openxer` entry stays as the file had it, in
/// every layout: one line, two or four spaces, tabs, Windows line ends.
#[test]
fn json_connect_and_disconnect_keep_every_other_byte() {
    let layouts = [
        r#"{"mcpServers":{"zeta":{"command":"z.exe","args":["-x"]}},"theme":"dark"}"#.to_string(),
        "{\n  \"mcpServers\": {\n    \"zeta\": {\n      \"command\": \"z.exe\"\n    }\n  },\n  \"globalShortcut\": \"\"\n}\n".to_string(),
        "{\n    \"x\": [1,2,  3],\n    \"mcpServers\": {\"a\": {}, \"b\": {\"command\": \"b\"}}\n}".to_string(),
        "{\n\t\"mcpServers\": {\n\t\t\"zeta\": {\"command\": \"z\"}\n\t}\n}\n".to_string(),
        "{\r\n  \"mcpServers\": {\r\n    \"zeta\": {\"command\": \"z\"}\r\n  }\r\n}\r\n".to_string(),
        "\u{feff}{ \"mcpServers\" : { \"zeta\" : { } } }".to_string(),
    ];
    for text in layouts {
        let out = json_connect(&text, &cli()).unwrap();
        assert_eq!(json_entry(&out, &cli(), &yes), Entry::Connected, "{out}");
        assert!(Format::Json.only_entry_changed(&text, &out));
        // Taking the entry out again gives the original back, byte for byte.
        assert_eq!(json_disconnect(&out).unwrap(), text, "{out}");
        // The new entry follows the file's line ends.
        if text.contains("\r\n") {
            assert!(!out.replace("\r\n", "").contains('\n'), "{out:?}");
        }
        // Connecting again changes nothing.
        assert_eq!(json_connect(&out, &cli()).unwrap(), out);
    }
    // A two-space file gets a two-space entry, as Claude Desktop writes it.
    let pretty =
        "{\n  \"mcpServers\": {\n    \"zeta\": {\n      \"command\": \"z.exe\"\n    }\n  }\n}\n";
    let out = json_connect(pretty, &cli()).unwrap();
    let expected = format!(
        "{{\n  \"mcpServers\": {{\n    \"zeta\": {{\n      \"command\": \"z.exe\"\n    }},\n    \"openxer\": {{\n      \"command\": {},\n      \"args\": [\n        \"mcp\"\n      ]\n    }}\n  }}\n}}\n",
        json!(cli().display().to_string())
    );
    assert_eq!(out, expected);
    // A file with no servers yet keeps its settings as written.
    let settings = "{\n  \"theme\": \"dark\",\n  \"x\": {\"y\": 1}\n}\n";
    let out = json_connect(settings, &cli()).unwrap();
    assert!(out.starts_with("{\n  \"theme\": \"dark\",\n  \"x\": {\"y\": 1},\n  \"mcpServers\": {\n    \"openxer\": {"), "{out}");
    assert!(Format::Json.only_entry_changed(settings, &out));
}

#[test]
fn json_disconnect_of_the_first_or_only_server() {
    let first =
        r#"{"mcpServers":{"openxer":{"command":"x","args":["mcp"]},"other":{"command":"o"}}}"#;
    assert_eq!(
        json_disconnect(first).unwrap(),
        r#"{"mcpServers":{"other":{"command":"o"}}}"#
    );
    let only = "{\n  \"mcpServers\": {\n    \"openxer\": {\"command\": \"x\"}\n  }\n}\n";
    assert_eq!(
        json_disconnect(only).unwrap(),
        "{\n  \"mcpServers\": {}\n}\n"
    );
    // An entry that is not an object is replaced whole on connect.
    let odd = r#"{"mcpServers":{"openxer":5,"b":true}}"#;
    let out = json_connect(odd, &cli()).unwrap();
    assert_eq!(json_entry(&out, &cli(), &yes), Entry::Connected);
    assert!(out.ends_with(r#","b":true}}"#), "{out}");
    // An entry missing args gets them after its last field.
    let partial = format!(
        r#"{{"mcpServers":{{"openxer":{{"command":{},"env":{{"A":"1"}}}}}}}}"#,
        json!(cli().display().to_string())
    );
    let out = json_connect(&partial, &cli()).unwrap();
    assert_eq!(json_entry(&out, &cli(), &yes), Entry::Connected);
    assert!(out.contains(r#""env":{"A":"1"},"args":["mcp"]"#), "{out}");
}

/// Paths that are hard to quote read back exactly from both formats; the
/// TOML side writes a literal string whenever the path has no single quote,
/// so no backslash is ever an escape.
#[test]
fn hard_paths_read_back_exactly_from_json_and_toml() {
    let paths = [
        r"C:\Program Files\OpenXER\openxer-cli.exe",
        r"C:\Users\Zoë Müller\Plant Ünit 2 (Zürich)\openxer-cli.exe",
        r"C:\odd\name\",
        r"C:\Users\O'Brien\OpenXER\openxer-cli.exe",
        r"\\server\share\x\u0041\n\t\openxer-cli.exe",
        "C:\\Tab\there\\openxer-cli.exe",
    ];
    for path in paths {
        let cli = PathBuf::from(path);
        let toml = toml_connect(CODEX, &cli).unwrap();
        let doc: toml_edit::DocumentMut = toml.parse().unwrap();
        assert_eq!(
            doc["mcp_servers"]["openxer"]["command"].as_str(),
            Some(path),
            "{toml}"
        );
        // TOML literal strings may hold a tab, never a single quote.
        let literal = !path.contains('\'');
        assert_eq!(
            toml.contains(&format!("command = '{path}'")),
            literal,
            "{toml}"
        );
        assert_eq!(toml_entry(&toml, &cli, &yes), Entry::Connected);
        for base in ["", "{}", "{\n  \"a\": 1\n}\n"] {
            let json = json_connect(base, &cli).unwrap();
            assert_eq!(
                parse(&json)["mcpServers"]["openxer"]["command"].as_str(),
                Some(path)
            );
            assert_eq!(json_entry(&json, &cli, &yes), Entry::Connected);
        }
    }
}

#[test]
fn another_copy_is_told_apart_from_a_missing_file() {
    // An absolute path on this system (a relative one is always stale).
    #[cfg(windows)]
    let other = r"D:\Portable OpenXER\openxer-cli.exe";
    #[cfg(not(windows))]
    let other = "/opt/Portable OpenXER/openxer-cli";
    let text = format!(
        r#"{{"mcpServers":{{"openxer":{{"command":{},"args":["mcp"]}}}}}}"#,
        json!(other)
    );
    // The other copy exists, without the installer's marker: portable.
    let exists_no_marker = |p: &Path| !p.ends_with(INSTALLED_MARKER);
    assert_eq!(
        json_entry(&text, &cli(), &exists_no_marker),
        Entry::OtherCopy {
            command: other.into(),
            installed: false
        }
    );
    assert_eq!(
        json_entry(&text, &cli(), &yes),
        Entry::OtherCopy {
            command: other.into(),
            installed: true
        }
    );
    // It no longer exists: repair.
    let only_ours = |p: &Path| p == cli();
    assert_eq!(
        json_entry(&text, &cli(), &only_ours),
        Entry::Stale {
            command: other.into()
        }
    );
    // Another copy with other arguments, or a bare name: repair.
    let serve = text.replace("[\"mcp\"]", "[\"serve\"]");
    assert!(matches!(
        json_entry(&serve, &cli(), &yes),
        Entry::Stale { .. }
    ));
    let bare = r#"{"mcpServers":{"openxer":{"command":"openxer-cli","args":["mcp"]}}}"#;
    assert!(matches!(
        json_entry(bare, &cli(), &yes),
        Entry::Stale { .. }
    ));
    assert!(Entry::OtherCopy {
        command: String::new(),
        installed: false
    }
    .works());
    assert!(!Entry::Stale {
        command: String::new()
    }
    .works());
}

#[test]
fn each_state_offers_one_action() {
    use Action::*;
    let stale = Entry::Stale {
        command: "x".into(),
    };
    let other = Entry::OtherCopy {
        command: "x".into(),
        installed: false,
    };
    let bad = Entry::Malformed {
        message: "m".into(),
    };
    for (client, found, entry, expected) in [
        (Client::ClaudeDesktop, true, Entry::NotConnected, Connect),
        (
            Client::ClaudeDesktop,
            false,
            Entry::NotConnected,
            NotInstalled,
        ),
        (Client::ClaudeDesktop, true, Entry::Connected, Connected),
        (Client::ClaudeDesktop, false, Entry::Connected, Connected),
        (Client::Codex, true, other.clone(), UseThisCopy),
        (Client::Codex, true, stale.clone(), Repair),
        (Client::Codex, false, stale.clone(), Repair),
        (Client::Codex, true, bad.clone(), FixFile),
        (Client::ClaudeCode, true, Entry::NotConnected, CopyCommand),
        (Client::ClaudeCode, false, Entry::NotConnected, NotInstalled),
    ] {
        assert_eq!(
            action(client, found, &entry),
            expected,
            "{client:?} {entry:?}"
        );
    }
    assert!(Connect.in_connect_all() && Repair.in_connect_all());
    for a in [Connected, UseThisCopy, NotInstalled, FixFile, CopyCommand] {
        assert!(!a.in_connect_all(), "{a:?}");
    }
}

#[test]
fn chatgpt_is_another_name_for_the_codex_config() {
    assert_eq!(Client::parse("chatgpt"), Some(Client::Codex));
    assert_eq!(Client::parse("codex"), Some(Client::Codex));
    assert_eq!(Client::Codex.name(), "ChatGPT desktop and Codex");
    assert_eq!(Client::Codex.id(), "codex");
    assert_eq!(
        Client::ALL[1],
        Client::Codex,
        "listed right after Claude Desktop"
    );
    assert!(Client::ClaudeDesktop
        .next_step()
        .starts_with("Quit Claude Desktop from its tray icon"));
    assert!(Client::ClaudeDesktop
        .next_step()
        .ends_with("\"Use OpenXER to inspect my schedule.\""));
    let codex = Client::Codex.next_step();
    assert!(codex.starts_with("Restart the app"), "{codex}");
    assert!(codex.contains("Work or Codex mode") && codex.contains("plain Chat may not use tools"));
    for c in Client::ALL {
        let s = c.next_step();
        assert!(!s.contains('\u{2014}') && !s.contains('\u{2013}'));
    }
}

#[test]
fn json_connect_keeps_other_servers_settings_and_order() {
    let text = r#"{
  "theme": "dark",
  "mcpServers": {
    "zeta": {"command": "z.exe", "args": ["--fast"], "env": {"K": "1"}},
    "alpha": {"command": "a.exe"}
  },
  "globalShortcut": "Ctrl+Space",
  "nested": {"b": 2, "a": [1, 2.5, null, true]}
}"#;
    let out = json_connect(text, &cli()).unwrap();
    let value = parse(&out);
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["globalShortcut"], "Ctrl+Space");
    assert_eq!(
        value["nested"],
        parse(r#"{"b": 2, "a": [1, 2.5, null, true]}"#)
    );
    assert_eq!(
        value["mcpServers"]["zeta"],
        parse(r#"{"command": "z.exe", "args": ["--fast"], "env": {"K": "1"}}"#)
    );
    assert_eq!(
        value["mcpServers"]["alpha"],
        parse(r#"{"command": "a.exe"}"#)
    );
    // File order is kept: keys appear in the text in their original order,
    // ours last.
    let at = |k: &str| out.find(k).unwrap();
    assert!(at("\"theme\"") < at("\"mcpServers\""));
    assert!(at("\"mcpServers\"") < at("\"globalShortcut\""));
    assert!(at("\"zeta\"") < at("\"alpha\""));
    assert!(at("\"alpha\"") < at("\"openxer\""));
    assert!(at("\"b\"") < at("\"a\": ["));
    assert!(Format::Json.only_entry_changed(text, &out));
}

#[test]
fn json_reconnect_updates_a_stale_path_and_keeps_extra_fields() {
    let text = r#"{"mcpServers":{"openxer":{"command":"D:\\old\\openxer-cli.exe","args":["mcp"],"env":{"X":"1"}},"other":{"command":"o"}}}"#;
    assert_eq!(
        json_entry(text, &cli(), &|p: &Path| p == cli()),
        Entry::Stale {
            command: r"D:\old\openxer-cli.exe".into()
        }
    );
    let out = json_connect(text, &cli()).unwrap();
    let value = parse(&out);
    assert_eq!(
        value["mcpServers"]["openxer"]["command"],
        cli().display().to_string()
    );
    assert_eq!(value["mcpServers"]["openxer"]["env"], json!({"X":"1"}));
    assert_eq!(value["mcpServers"]["other"], json!({"command":"o"}));
    assert_eq!(json_entry(&out, &cli(), &yes), Entry::Connected);
}

#[test]
fn json_disconnect_removes_only_ours() {
    let text = r#"{"a":1,"mcpServers":{"other":{"command":"o"},"openxer":{"command":"x","args":["mcp"]}}}"#;
    let out = json_disconnect(text).unwrap();
    assert_eq!(
        parse(&out),
        json!({"a":1,"mcpServers":{"other":{"command":"o"}}})
    );
    assert_eq!(json_entry(&out, &cli(), &yes), Entry::NotConnected);
    assert!(Format::Json.only_entry_changed(text, &out));
    // Nothing to remove: the text comes back byte for byte.
    let plain = "{ \"a\" : 1 }";
    assert_eq!(json_disconnect(plain).unwrap(), plain);
}

#[test]
fn json_malformed_input_is_refused() {
    for (text, why) in [
        ("{\"mcpServers\": {", "not valid JSON"),
        ("[1, 2]", "not a JSON object"),
        ("{\"mcpServers\": []}", "is not an object"),
        ("// comment\n{}", "not valid JSON"),
    ] {
        let error = json_connect(text, &cli()).unwrap_err();
        assert!(error.contains(why), "{text:?}: {error}");
        assert!(json_disconnect(text).is_err());
        assert!(matches!(
            json_entry(text, &cli(), &yes),
            Entry::Malformed { .. }
        ));
    }
}

#[test]
fn json_status_detection() {
    let entry = |command: &str, args: &str| {
        format!(
            r#"{{"mcpServers":{{"openxer":{{"command":{},"args":{args}}}}}}}"#,
            json!(command)
        )
    };
    let ours = cli().display().to_string();
    assert_eq!(json_entry("{}", &cli(), &yes), Entry::NotConnected);
    assert_eq!(
        json_entry(&entry(&ours, r#"["mcp"]"#), &cli(), &yes),
        Entry::Connected
    );
    // Case and separators do not matter on Windows.
    if cfg!(windows) {
        let other_case = ours.to_uppercase().replace('\\', "/");
        assert_eq!(
            json_entry(&entry(&other_case, r#"["mcp"]"#), &cli(), &yes),
            Entry::Connected
        );
    }
    // Our path, but the executable is missing: out of date.
    assert!(matches!(
        json_entry(&entry(&ours, r#"["mcp"]"#), &cli(), &no),
        Entry::Stale { .. }
    ));
    // Other arguments: out of date.
    assert!(matches!(
        json_entry(&entry(&ours, r#"["serve"]"#), &cli(), &yes),
        Entry::Stale { .. }
    ));
    assert!(matches!(
        json_entry(&entry(&ours, "[]"), &cli(), &yes),
        Entry::Stale { .. }
    ));
    assert!(matches!(
        json_entry(r#"{"mcpServers":{"openxer":5}}"#, &cli(), &yes),
        Entry::Stale { .. }
    ));
}

// ------------------------------------------------------------------- TOML

const CODEX: &str = r#"# My Codex settings
model = "gpt-x" # inline comment
approval_policy = "on-request"

[profiles.fast]
model = "small"

# servers below
[mcp_servers.docs]
command = "npx"
args = ["-y", "docs-server"] # keep me
"#;

#[test]
fn toml_connect_into_missing_or_empty_config() {
    for text in ["", "\n"] {
        let out = toml_connect(text, &cli()).unwrap();
        assert!(out.contains("[mcp_servers.openxer]"), "{out}");
        assert_eq!(toml_entry(&out, &cli(), &yes), Entry::Connected);
        assert!(
            !out.contains("[mcp_servers]\n"),
            "no empty parent header: {out}"
        );
    }
}

#[test]
fn toml_connect_preserves_comments_and_formatting() {
    let out = toml_connect(CODEX, &cli()).unwrap();
    assert!(
        out.starts_with(CODEX),
        "everything already there is kept byte for byte:\n{out}"
    );
    let added = &out[CODEX.len()..];
    assert!(added.contains("[mcp_servers.openxer]"), "{added}");
    assert!(added.contains("command = "), "{added}");
    assert!(added.contains(r#"args = ["mcp"]"#), "{added}");
    assert_eq!(toml_entry(&out, &cli(), &yes), Entry::Connected);
    assert!(Format::Toml.only_entry_changed(CODEX, &out));
    // The command reads back as the exact path.
    let doc: toml_edit::DocumentMut = out.parse().unwrap();
    assert_eq!(
        doc["mcp_servers"]["openxer"]["command"].as_str(),
        Some(cli().display().to_string().as_str())
    );
}

#[test]
fn toml_reconnect_updates_a_stale_path_in_place() {
    let text = format!(
        "{CODEX}\n[mcp_servers.openxer]\n# ours\ncommand = 'D:\\old\\openxer-cli.exe'\nargs = [\"mcp\"]\nstartup_timeout_sec = 30\n\n[after]\nx = 1\n"
    );
    assert_eq!(
        toml_entry(&text, &cli(), &|p: &Path| p == cli()),
        Entry::Stale {
            command: r"D:\old\openxer-cli.exe".into()
        }
    );
    let out = toml_connect(&text, &cli()).unwrap();
    assert_eq!(toml_entry(&out, &cli(), &yes), Entry::Connected);
    assert!(out.contains("# ours"));
    assert!(out.contains("startup_timeout_sec = 30"));
    assert!(out.starts_with(CODEX));
    assert!(out.find("[mcp_servers.openxer]").unwrap() < out.find("[after]").unwrap());
    assert!(Format::Toml.only_entry_changed(&text, &out));
}

#[test]
fn toml_disconnect_removes_only_ours() {
    let connected = toml_connect(CODEX, &cli()).unwrap();
    let out = toml_disconnect(&connected).unwrap();
    assert_eq!(toml_entry(&out, &cli(), &yes), Entry::NotConnected);
    assert!(out.contains("[mcp_servers.docs]"));
    assert!(out.contains("# keep me"));
    assert!(out.contains("# My Codex settings"));
    assert_eq!(out.trim_end(), CODEX.trim_end());
    assert_eq!(toml_disconnect(CODEX).unwrap(), CODEX, "nothing to remove");
}

#[test]
fn toml_inline_server_table() {
    let text = "mcp_servers = { docs = { command = \"npx\" } }\n";
    let out = toml_connect(text, &cli()).unwrap();
    assert_eq!(toml_entry(&out, &cli(), &yes), Entry::Connected);
    assert!(out.contains("docs = { command = \"npx\" }"), "{out}");
    let back = toml_disconnect(&out).unwrap();
    assert_eq!(toml_entry(&back, &cli(), &yes), Entry::NotConnected);
    assert!(back.contains("docs"));
}

#[test]
fn toml_malformed_input_is_refused() {
    for (text, why) in [
        ("model = \n", "not valid TOML"),
        ("[a\nb = 1", "not valid TOML"),
        ("mcp_servers = 3\n", "is not a table"),
    ] {
        let error = toml_connect(text, &cli()).unwrap_err();
        assert!(error.contains(why), "{text:?}: {error}");
        assert!(toml_disconnect(text).is_err());
        assert!(matches!(
            toml_entry(text, &cli(), &yes),
            Entry::Malformed { .. }
        ));
    }
}

// ------------------------------------------------------------ commands

#[test]
fn claude_code_command_quotes_paths_with_spaces() {
    assert_eq!(
        claude_code_command(&cli()),
        r#"claude mcp add --scope user openxer -- "C:\Program Files\OpenXER\openxer-cli.exe" mcp"#
    );
    assert_eq!(
        claude_code_command(Path::new(r"C:\x\openxer-cli.exe")),
        r#"claude mcp add --scope user openxer -- "C:\x\openxer-cli.exe" mcp"#
    );
}

#[test]
fn setup_snippet_is_generic_mcp_json() {
    let value = parse(&setup_snippet(&cli()));
    assert_eq!(
        value,
        json!({"mcpServers":{"openxer":{"command":cli().display().to_string(),"args":["mcp"]}}})
    );
}

#[test]
fn timestamps_are_utc_civil_dates() {
    assert_eq!(timestamp(UNIX_EPOCH), "19700101-000000");
    // 2026-11-07 18:15:02 UTC.
    assert_eq!(
        timestamp(UNIX_EPOCH + Duration::from_secs(1_794_075_302)),
        "20261107-181502"
    );
    assert_eq!(
        timestamp(UNIX_EPOCH + Duration::from_secs(951_782_400)),
        "20000229-000000"
    );
}

// ---------------------------------------------------------- file layer

/// A fake user profile in a temporary folder: APPDATA, LOCALAPPDATA, home
/// and PATH all inside it, and a stand-in openxer-cli.exe.
struct Fake {
    _dir: tempfile::TempDir,
    root: PathBuf,
    env: Env,
}

fn fake() -> Fake {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for sub in ["Roaming", "Local", "home", "bin", "app"] {
        fs::create_dir_all(root.join(sub)).unwrap();
    }
    let cli = root.join("app").join(CLI_NAME);
    fs::write(&cli, b"stand-in").unwrap();
    let env = Env {
        appdata: Some(root.join("Roaming")),
        local_appdata: Some(root.join("Local")),
        home: Some(root.join("home")),
        codex_home: None,
        path: Some(root.join("bin").into_os_string()),
        cli: Some(cli),
    };
    Fake {
        _dir: dir,
        root,
        env,
    }
}

fn backups(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().contains(".openxer-backup-"))
        .collect()
}

#[test]
fn nothing_installed_is_not_found_and_not_connected() {
    let f = fake();
    for s in status(&f.env) {
        assert!(!s.found, "{:?}", s.client);
        assert_eq!(s.entry, Entry::NotConnected);
    }
    // Claude Desktop still has a place to write: the classic folder.
    assert_eq!(
        f.env.claude_desktop_files(),
        vec![f
            .root
            .join("Roaming")
            .join("Claude")
            .join(CLAUDE_DESKTOP_FILE)]
    );
    assert_eq!(
        f.env.codex_file(),
        Some(f.root.join("home").join(".codex").join("config.toml"))
    );
}

#[test]
fn claude_desktop_round_trip_with_backup() {
    let f = fake();
    let dir = f.root.join("Roaming").join("Claude");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join(CLAUDE_DESKTOP_FILE);
    let original =
        "{\n  \"mcpServers\": {\n    \"other\": {\"command\": \"o.exe\"}\n  },\n  \"x\": true\n}\n";
    fs::write(&file, original).unwrap();
    assert!(f.env.found(Client::ClaudeDesktop));

    let changes = apply(&f.env, Client::ClaudeDesktop, false, false).unwrap();
    assert_eq!(changes.len(), 1);
    assert!(changes[0].changed && !changes[0].created);
    let backup = changes[0].backup.clone().expect("backup written");
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("claude_desktop_config.json.openxer-backup-"));
    assert_eq!(
        client_status(&f.env, Client::ClaudeDesktop).entry,
        Entry::Connected
    );
    let written = parse(&fs::read_to_string(&file).unwrap());
    assert_eq!(written["mcpServers"]["other"], json!({"command":"o.exe"}));
    assert_eq!(written["x"], true);
    assert_eq!(
        finish_message(Client::ClaudeDesktop, false, &changes, false),
        format!("Connected. {}", Client::ClaudeDesktop.next_step())
    );
    // The file differs from the original only by the added entry.
    let written_text = fs::read_to_string(&file).unwrap();
    assert!(
        written_text.starts_with(
            "{\n  \"mcpServers\": {\n    \"other\": {\"command\": \"o.exe\"},\n    \"openxer\": {"
        ),
        "{written_text}"
    );
    assert!(
        written_text.ends_with("\n  },\n  \"x\": true\n}\n"),
        "{written_text}"
    );
    // No temporary files are left behind.
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());

    // Connecting again changes nothing and makes no second backup.
    let again = apply(&f.env, Client::ClaudeDesktop, false, false).unwrap();
    assert!(!again[0].changed && again[0].backup.is_none());
    assert_eq!(backups(&dir).len(), 1);

    let removed = apply(&f.env, Client::ClaudeDesktop, true, false).unwrap();
    assert!(removed[0].changed && removed[0].backup.is_some());
    assert_eq!(backups(&dir).len(), 2);
    assert_eq!(
        client_status(&f.env, Client::ClaudeDesktop).entry,
        Entry::NotConnected
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original,
        "byte for byte"
    );
}

#[test]
fn connect_creates_a_missing_folder_and_file() {
    let f = fake();
    let changes = apply(&f.env, Client::Codex, false, false).unwrap();
    let file = f.root.join("home").join(".codex").join("config.toml");
    assert_eq!(changes[0].path, file);
    assert!(changes[0].created && changes[0].backup.is_none());
    assert_eq!(client_status(&f.env, Client::Codex).entry, Entry::Connected);
    // Disconnecting a client with no file writes nothing.
    let none = apply(&f.env, Client::ClaudeDesktop, true, false).unwrap();
    assert!(!none[0].changed);
    assert!(!f.root.join("Roaming").join("Claude").exists());
}

#[test]
fn codex_home_overrides_the_profile_folder() {
    let mut f = fake();
    let home = f.root.join("codex-elsewhere");
    fs::create_dir_all(&home).unwrap();
    f.env.codex_home = Some(home.clone());
    assert!(f.env.found(Client::Codex));
    assert_eq!(f.env.codex_file(), Some(home.join("config.toml")));
}

#[test]
fn malformed_config_is_refused_and_left_unchanged() {
    let f = fake();
    let dir = f.root.join("home").join(".codex");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("config.toml");
    let broken = "model = \"x\"\n[mcp_servers\n";
    fs::write(&file, broken).unwrap();
    assert!(matches!(
        client_status(&f.env, Client::Codex).entry,
        Entry::Malformed { .. }
    ));
    for disconnect in [false, true] {
        let error = apply(&f.env, Client::Codex, disconnect, false).unwrap_err();
        assert_eq!(error.code, "config_malformed");
        assert!(error.message.contains("Copy Setup"), "{}", error.message);
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), broken);
    assert!(
        backups(&dir).is_empty(),
        "no backup of a file that was not changed"
    );
}

#[test]
fn dry_run_writes_nothing() {
    let f = fake();
    let changes = apply(&f.env, Client::Codex, false, true).unwrap();
    assert!(changes[0].changed);
    assert!(changes[0]
        .preview
        .as_deref()
        .unwrap()
        .contains("[mcp_servers.openxer]"));
    assert!(!f.root.join("home").join(".codex").exists());
    assert_eq!(
        finish_message(Client::Codex, false, &changes, true),
        "Dry run: nothing was written."
    );
}

#[test]
fn missing_cli_is_a_clear_error() {
    let mut f = fake();
    f.env.cli = Some(f.root.join("nowhere").join(CLI_NAME));
    let error = apply(&f.env, Client::Codex, false, false).unwrap_err();
    assert_eq!(error.code, "cli_missing");
    assert!(error.message.contains("nowhere"));
    assert!(!f.root.join("home").join(".codex").exists());
    let error = test_connection(&f.root.join("nowhere").join(CLI_NAME), TEST_TIMEOUT).unwrap_err();
    assert_eq!(error.code, "cli_missing");
}

#[test]
fn store_install_is_detected_and_kept_in_step() {
    let f = fake();
    let package = f
        .root
        .join("Local")
        .join("Packages")
        .join("Claude_pzs8sxrjxfjjc");
    fs::create_dir_all(&package).unwrap();
    // An unrelated package is ignored.
    fs::create_dir_all(
        f.root
            .join("Local")
            .join("Packages")
            .join("ClaudeLike.Other_1"),
    )
    .unwrap();
    assert!(f.env.found(Client::ClaudeDesktop));
    let store_file = package
        .join("LocalCache")
        .join("Roaming")
        .join("Claude")
        .join(CLAUDE_DESKTOP_FILE);
    // Only the Store install: only its copy.
    assert_eq!(f.env.claude_desktop_files(), vec![store_file.clone()]);

    // With the classic install too, both files, and a Store copy that does
    // not exist yet starts from the classic file's servers.
    let classic_dir = f.root.join("Roaming").join("Claude");
    fs::create_dir_all(&classic_dir).unwrap();
    let classic = classic_dir.join(CLAUDE_DESKTOP_FILE);
    fs::write(&classic, r#"{"mcpServers":{"other":{"command":"o"}}}"#).unwrap();
    assert_eq!(
        f.env.claude_desktop_files(),
        vec![store_file.clone(), classic.clone()]
    );
    let changes = apply(&f.env, Client::ClaudeDesktop, false, false).unwrap();
    assert_eq!(changes.len(), 2);
    assert!(changes[0].created);
    let store_value = parse(&fs::read_to_string(&store_file).unwrap());
    assert_eq!(store_value["mcpServers"]["other"], json!({"command":"o"}));
    assert_eq!(
        client_status(&f.env, Client::ClaudeDesktop).entry,
        Entry::Connected
    );

    // One file out of date makes the client out of date.
    fs::write(
        &store_file,
        r#"{"mcpServers":{"openxer":{"command":"old.exe","args":["mcp"]}}}"#,
    )
    .unwrap();
    assert!(matches!(
        client_status(&f.env, Client::ClaudeDesktop).entry,
        Entry::Stale { .. }
    ));
}

#[test]
fn claude_code_is_found_on_path_and_never_edited() {
    let f = fake();
    assert!(!f.env.found(Client::ClaudeCode));
    let name = if cfg!(windows) {
        "claude.cmd"
    } else {
        "claude"
    };
    fs::write(f.root.join("bin").join(name), b"").unwrap();
    assert!(f.env.found(Client::ClaudeCode));
    let s = client_status(&f.env, Client::ClaudeCode);
    assert!(s.files.is_empty());
    assert!(s
        .command
        .unwrap()
        .starts_with("claude mcp add --scope user openxer -- \""));
    assert_eq!(
        apply(&f.env, Client::ClaudeCode, false, false)
            .unwrap_err()
            .code,
        "unsupported"
    );
}

#[test]
fn backup_names_do_not_collide() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.toml");
    let first = backup_path(&file, "20261107-000000");
    fs::write(&first, "").unwrap();
    let second = backup_path(&file, "20261107-000000");
    assert_ne!(first, second);
    assert!(second.to_string_lossy().ends_with("-2"));
}

#[test]
fn resolve_cli_beside_the_executable() {
    if std::env::var_os("OPENXER_CLI").is_some() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("OpenXER.exe");
    assert_eq!(resolve_cli(Some(&exe)), Some(dir.path().join(CLI_NAME)));
    let deps = dir.path().join("deps");
    fs::create_dir_all(&deps).unwrap();
    assert_eq!(
        resolve_cli(Some(&deps.join("runner.exe"))),
        Some(dir.path().join(CLI_NAME)),
        "a test runner in deps looks one folder up"
    );
    assert_eq!(resolve_cli(None), None);
}

// ------------------------------------------- a fresh install, end to end

/// A second copy of OpenXER inside the fake profile; `installed` puts the
/// installer's marker beside it.
fn other_copy(f: &Fake, name: &str, installed: bool) -> PathBuf {
    let dir = f.root.join(name);
    fs::create_dir_all(&dir).unwrap();
    let cli = dir.join(CLI_NAME);
    fs::write(&cli, b"another copy").unwrap();
    if installed {
        fs::write(dir.join(INSTALLED_MARKER), b"").unwrap();
    }
    cli
}

fn claude_classic(f: &Fake) -> PathBuf {
    let dir = f.root.join("Roaming").join("Claude");
    fs::create_dir_all(&dir).unwrap();
    dir.join(CLAUDE_DESKTOP_FILE)
}

fn codex_config(f: &Fake) -> PathBuf {
    let dir = f.root.join("home").join(".codex");
    fs::create_dir_all(&dir).unwrap();
    dir.join("config.toml")
}

#[test]
fn an_empty_existing_config_is_connected_and_backed_up() {
    let f = fake();
    for (client, file) in [
        (Client::ClaudeDesktop, claude_classic(&f)),
        (Client::Codex, codex_config(&f)),
    ] {
        fs::write(&file, "").unwrap();
        let s = client_status(&f.env, client);
        assert_eq!((s.entry, s.action), (Entry::NotConnected, Action::Connect));
        let changes = apply(&f.env, client, false, false).unwrap();
        assert!(changes[0].changed && !changes[0].created);
        let backup = changes[0]
            .backup
            .clone()
            .expect("a backup, even of an empty file");
        assert_eq!(fs::read_to_string(backup).unwrap(), "");
        assert_eq!(client_status(&f.env, client).action, Action::Connected);
    }
}

#[test]
fn a_read_only_config_is_refused_and_left_alone() {
    let f = fake();
    let file = claude_classic(&f);
    let original = r#"{"mcpServers":{"other":{"command":"o"}}}"#;
    fs::write(&file, original).unwrap();
    let mut perms = fs::metadata(&file).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&file, perms.clone()).unwrap();
    let error = apply(&f.env, Client::ClaudeDesktop, false, false).unwrap_err();
    assert_eq!(error.code, "config_read_only");
    assert!(error.message.contains("read-only"), "{}", error.message);
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    assert!(backups(file.parent().unwrap()).is_empty());
    // A dry run still shows what would change.
    assert!(apply(&f.env, Client::ClaudeDesktop, false, true).unwrap()[0].changed);
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(&file, perms).unwrap();
}

#[test]
fn use_this_copy_and_repair_repoint_the_entry() {
    let f = fake();
    let portable = other_copy(&f, "portable", false);
    let file = claude_classic(&f);
    let text = format!(
        "{{\n  \"mcpServers\": {{\n    \"openxer\": {{\n      \"command\": {},\n      \"args\": [\"mcp\"],\n      \"env\": {{\"K\": \"v\"}}\n    }}\n  }}\n}}\n",
        json!(portable.display().to_string())
    );
    fs::write(&file, &text).unwrap();
    let s = client_status(&f.env, Client::ClaudeDesktop);
    assert_eq!(
        s.entry,
        Entry::OtherCopy {
            command: portable.display().to_string(),
            installed: false
        }
    );
    assert_eq!(s.action, Action::UseThisCopy);
    // Use this copy: only the command changes; env stays.
    apply(&f.env, Client::ClaudeDesktop, false, false).unwrap();
    let after = fs::read_to_string(&file).unwrap();
    assert_eq!(
        after,
        text.replace(
            &json!(portable.display().to_string()).to_string(),
            &json!(f.env.cli.as_ref().unwrap().display().to_string()).to_string()
        )
    );
    assert_eq!(
        client_status(&f.env, Client::ClaudeDesktop).action,
        Action::Connected
    );

    // The other copy is removed while Codex points at it: repair.
    let installed = other_copy(&f, "installed", true);
    let codex = codex_config(&f);
    fs::write(
        &codex,
        format!(
            "[mcp_servers.openxer]\ncommand = '{}'\nargs = [\"mcp\"]\n",
            installed.display()
        ),
    )
    .unwrap();
    assert!(matches!(
        client_status(&f.env, Client::Codex).entry,
        Entry::OtherCopy {
            installed: true,
            ..
        }
    ));
    fs::remove_file(&installed).unwrap();
    let s = client_status(&f.env, Client::Codex);
    assert_eq!(s.action, Action::Repair);
    apply(&f.env, Client::Codex, false, false).unwrap();
    assert_eq!(
        client_status(&f.env, Client::Codex).action,
        Action::Connected
    );
    let text = fs::read_to_string(&codex).unwrap();
    assert!(
        text.contains(&format!(
            "command = '{}'",
            f.env.cli.as_ref().unwrap().display()
        )),
        "a literal string: {text}"
    );
}

#[test]
fn connect_all_connects_the_unconnected_and_never_repoints_a_working_one() {
    let f = fake();
    // Claude Desktop found (classic), nothing connected.
    let claude = claude_classic(&f);
    // Codex found, connected to another copy that works.
    let other = other_copy(&f, "installed", true);
    let codex = codex_config(&f);
    let codex_text = format!(
        "# mine\n[mcp_servers.openxer]\ncommand = '{}'\nargs = [\"mcp\"]\n",
        other.display()
    );
    fs::write(&codex, &codex_text).unwrap();
    assert_eq!(
        connect_all_targets(&status(&f.env)),
        vec![Client::ClaudeDesktop]
    );
    let dry = connect_all(&f.env, true);
    assert_eq!(dry.len(), 1);
    assert!(!claude.exists(), "a dry run writes nothing");
    let results = connect_all(&f.env, false);
    assert_eq!(results.len(), 1);
    assert!(results[0].ok, "{}", results[0].message);
    assert!(
        results[0].message.contains("tray icon"),
        "{}",
        results[0].message
    );
    assert_eq!(
        client_status(&f.env, Client::ClaudeDesktop).action,
        Action::Connected
    );
    assert_eq!(fs::read_to_string(&codex).unwrap(), codex_text, "untouched");
    // Everything done: nothing left to connect.
    assert!(connect_all(&f.env, false).is_empty());

    // One failing (unreadable Claude config) does not stop another.
    fs::write(&claude, "{ broken").unwrap();
    fs::write(&codex, "").unwrap();
    let results = connect_all(&f.env, false);
    assert_eq!(results.len(), 1, "a malformed file is not a target");
    assert_eq!(results[0].client, Client::Codex);
    assert_eq!(fs::read_to_string(&claude).unwrap(), "{ broken");
}

#[test]
fn the_chatgpt_and_codex_apps_are_found_by_their_store_packages() {
    let f = fake();
    let packages = f.root.join("Local").join("Packages");
    // The runtime package alone is not the app.
    fs::create_dir_all(packages.join("OpenAI.CodexPrimaryRuntime.v26-1_3k8")).unwrap();
    assert!(!f.env.found(Client::Codex));
    for name in [
        "OpenAI.ChatGPT-Desktop_2p2nqsd0c76g0",
        "OpenAI.Codex_2p2nqsd0c76g0",
    ] {
        fs::create_dir_all(packages.join(name)).unwrap();
        assert!(f.env.found(Client::Codex), "{name}");
        fs::remove_dir_all(packages.join(name)).unwrap();
    }
    fs::create_dir_all(f.root.join("Local").join("OpenAI").join("Codex")).unwrap();
    assert!(f.env.found(Client::Codex));
    // Found, so Connect, and the config is the shared one.
    let s = client_status(&f.env, Client::Codex);
    assert_eq!(s.action, Action::Connect);
    assert_eq!(
        s.files[0].path,
        f.root.join("home").join(".codex").join("config.toml")
    );
}

#[test]
fn both_claude_installs_with_other_servers_keep_them() {
    let f = fake();
    let package = f
        .root
        .join("Local")
        .join("Packages")
        .join("Claude_pzs8sxrjxfjjc");
    let store_dir = package.join("LocalCache").join("Roaming").join("Claude");
    fs::create_dir_all(&store_dir).unwrap();
    let store = store_dir.join(CLAUDE_DESKTOP_FILE);
    let store_text = "{\n  \"mcpServers\": {\n    \"files\": {\"command\": \"f.exe\"}\n  },\n  \"preferences\": {\"a\": 1}\n}\n";
    fs::write(&store, store_text).unwrap();
    let classic = claude_classic(&f);
    let classic_text = "{\"mcpServers\":{\"other\":{\"command\":\"o\"}}}";
    fs::write(&classic, classic_text).unwrap();
    let changes = apply(&f.env, Client::ClaudeDesktop, false, false).unwrap();
    assert_eq!(changes.len(), 2);
    assert!(changes.iter().all(|c| c.backup.is_some()));
    for (file, original) in [(&store, store_text), (&classic, classic_text)] {
        let now = fs::read_to_string(file).unwrap();
        assert_eq!(
            json_disconnect(&now).unwrap(),
            original,
            "byte for byte apart from ours"
        );
    }
}

#[test]
fn the_setup_prompt_names_this_copy_and_only_real_commands() {
    let prompt = setup_prompt(&cli());
    // The same quoted path as the Claude Code command.
    let quoted = format!("\"{}\"", cli().display());
    assert!(claude_code_command(&cli()).contains(&quoted));
    assert!(prompt.contains(&quoted), "{prompt}");
    for part in [
        "--help",
        "named openxer",
        "single argument mcp",
        "every command prints JSON",
        "connect test",
        "Working: N tools",
        "never changes an input file",
        "--output",
        "--dry-run",
        FIRST_PROMPT,
    ] {
        assert!(prompt.contains(part), "{part} in {prompt}");
    }
    // The cited flags and commands exist in --help.
    for command in ["connect test", "--dry-run", "--output", "mcp"] {
        assert!(crate::cli::HELP.contains(command), "{command} in --help");
    }
    assert!(!prompt.contains('\u{2014}') && !prompt.contains('\u{2013}'));
    // The CLI prints the same text.
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join(CLI_NAME);
    fs::write(&exe, b"").unwrap();
    let env = Env {
        cli: Some(exe.clone()),
        ..Env::default()
    };
    let out = run(&Request::Prompt, &env).unwrap();
    assert_eq!(out["prompt"], setup_prompt(&exe));
    assert!(
        run(&Request::Prompt, &Env::default()).is_err(),
        "no CLI, no prompt"
    );
}
