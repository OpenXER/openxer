use openxer_automation::{
    cli::{self, Invocation},
    Command, EditAction, RelationshipType,
};
use serde_json::Value;
use std::{path::PathBuf, process::Command as Process};

fn command(args: &[&str]) -> Command {
    match cli::parse(args.iter().copied()).unwrap() {
        Invocation::Run(command) => *command,
        other => panic!("Expected a command, got {other:?}"),
    }
}

#[test]
fn schedule_flags_preserve_paths_options_and_explicit_false() {
    let parsed = command(&[
        "schedule",
        "--input",
        "C:\\Project data\\café.xer",
        "--output",
        "C:\\Project data\\scheduled.xer",
        "--project",
        "-11432",
        "--options",
        r#"{"retained_logic":false,"progress_override":true,"must_finish":null}"#,
        "--data-date",
        "2026-11-04 08:00",
        "--level=false",
    ]);
    let Command::Schedule {
        input,
        output,
        project_id,
        options,
        ..
    } = parsed
    else {
        panic!("Expected schedule")
    };
    assert_eq!(input, PathBuf::from("C:\\Project data\\café.xer"));
    assert_eq!(output, PathBuf::from("C:\\Project data\\scheduled.xer"));
    assert_eq!(project_id, Some(-11432));
    assert_eq!(options.level_resources, Some(false));
    assert_eq!(options.data_date.as_deref(), Some("2026-11-04 08:00"));
    assert_eq!(options.progress_override, Some(true));
    assert_eq!(options.must_finish, Some(None));
    let Command::Schedule { options, .. } =
        command(&["schedule", "--input=a.xer", "--output=b.xer", "--level"])
    else {
        unreachable!()
    };
    assert_eq!(options.level_resources, Some(true));
}

#[test]
fn relationship_and_independent_pagination_are_typed() {
    let Command::Relationship {
        edit, workspace_id, ..
    } = command(&[
        "relationship",
        "update",
        "--input",
        "input.db",
        "--output",
        "output.db",
        "--workspace-id",
        "12",
        "--predecessor",
        "A 100",
        "--successor",
        "A 200",
        "--type",
        "fs",
        "--new-type",
        "SS",
        "--lag-minutes",
        "-90",
    ])
    else {
        panic!("Expected relationship")
    };
    assert_eq!(workspace_id, Some(12));
    assert_eq!(edit.action, EditAction::Update);
    assert!(matches!(edit.relationship_type, RelationshipType::FS));
    assert!(matches!(edit.new_type, Some(RelationshipType::SS)));
    assert_eq!(edit.lag_minutes, Some(-90));
    let Command::Load {
        offset,
        limit,
        relationship_offset,
        relationship_limit,
        ..
    } = command(&[
        "load",
        "--input",
        "input.db",
        "--offset",
        "100",
        "--limit",
        "50",
        "--relationship-offset",
        "200",
        "--relationship-limit",
        "25",
    ])
    else {
        panic!("Expected load")
    };
    assert_eq!(
        (offset, limit, relationship_offset, relationship_limit),
        (100, 50, 200, 25)
    );
}

#[test]
fn ambiguous_unknown_and_duplicate_arguments_are_rejected() {
    for args in [
        vec!["inspect", "--input", "a.xer", "--input=b.xer"],
        vec!["inspect", "--input", "a.xer", "--output", "b.xer"],
        vec![
            "load",
            "--input",
            "a.db",
            "--project",
            "1",
            "--workspace-id",
            "2",
        ],
        vec![
            "schedule",
            "--input",
            "a.xer",
            "--output",
            "b.xer",
            "--level",
            "--options",
            r#"{"level_resources":false}"#,
        ],
        vec![
            "schedule",
            "--input",
            "a.xer",
            "--output",
            "b.xer",
            "--options",
            r#"{"silent_unknown_option":true}"#,
        ],
        vec![
            "call",
            "--json",
            "{}",
            "--request-file",
            "must-not-be-read.json",
        ],
        vec!["mcp", "--input", "a.xer"],
        vec!["load", "--input", "a.db", "--offset", "-1"],
        vec!["export", "--input", "a.db", "--output"],
        vec!["workspace", "migrate", "--input", "a.db"],
        vec![
            "workspace",
            "migrate",
            "--input",
            "a.db",
            "--dry-run",
            "--output",
            "b.db",
        ],
        vec!["workspace", "list", "--input", "a.db"],
    ] {
        let error = cli::parse(args.clone()).unwrap_err();
        assert_eq!(error.code, "invalid_arguments", "{args:?}");
    }
}

#[test]
fn request_files_and_inline_calls_use_the_shared_typed_schema() {
    let directory = tempfile::tempdir().unwrap();
    let request = directory.path().join("request.json");
    std::fs::write(
        &request,
        r#"{"command":"load","input":"input.xer","project_id":7,"limit":3}"#,
    )
    .unwrap();
    let parsed = cli::parse(["call", "--request-file", request.to_str().unwrap()]).unwrap();
    let Invocation::Run(command) = parsed else {
        panic!("Expected typed request")
    };
    assert!(matches!(
        *command,
        Command::Load {
            project_id: Some(7),
            limit: 3,
            ..
        }
    ));
    assert!(matches!(command_from_inline(), Command::Inspect { .. }));
    assert!(cli::parse([
        "call",
        "--json",
        r#"{"command":"inspect","input":"a.xer","unexpected":true}"#
    ])
    .is_err());
}
fn command_from_inline() -> Command {
    command(&[
        "call",
        "--json",
        r#"{"command":"inspect","input":"input.xer"}"#,
    ])
}

#[test]
fn binary_keeps_machine_responses_clean_and_exit_codes_distinct() {
    let executable = env!("CARGO_BIN_EXE_openxer-cli");
    let help = Process::new(executable).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    assert!(String::from_utf8(help.stdout)
        .unwrap()
        .contains("--request-file"));
    let invalid = Process::new(executable)
        .args(["load", "--unknown"])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stderr.is_empty());
    let value: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["schema_version"], "1.0");
    assert_eq!(value["error"]["code"], "invalid_arguments");
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("does-not-exist.xer");
    let failed = Process::new(executable)
        .arg("inspect")
        .arg("--input")
        .arg(missing)
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stderr.is_empty());
    let value: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(value["ok"], false);
}

#[test]
fn workspace_migrate_flags_build_the_shared_request() {
    match command(&["workspace", "migrate", "--input", "w.db", "--dry-run"]) {
        Command::WorkspaceMigrate {
            input,
            output,
            dry_run,
        } => {
            assert_eq!(input, PathBuf::from("w.db"));
            assert_eq!(output, None);
            assert!(dry_run);
        }
        other => panic!("{other:?}"),
    }
    match command(&[
        "workspace",
        "migrate",
        "--input",
        "w.db",
        "--output",
        "n.db",
    ]) {
        Command::WorkspaceMigrate {
            output, dry_run, ..
        } => {
            assert_eq!(output, Some(PathBuf::from("n.db")));
            assert!(!dry_run);
        }
        other => panic!("{other:?}"),
    }
}

/// File errors name the file; empty text and bare true flags parse.
#[test]
fn file_errors_name_the_path_and_flags_take_empty_text_and_bare_booleans() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("no-such-schedule.xer");
    let e =
        openxer_automation::dispatch(command(&["inspect", "--input", missing.to_str().unwrap()]))
            .unwrap_err();
    assert!(e.message.contains("no-such-schedule.xer"), "{e}");
    let Command::Edit { operation, .. } = command(&[
        "set-feedback",
        "--input",
        "a.xer",
        "--dry-run",
        "--activity",
        "A1",
        "--notes",
        "",
        "--review-required",
    ]) else {
        panic!()
    };
    assert_eq!(operation["notes"], "");
    assert_eq!(operation["review_required"], true);
    let Command::Edit { operation, .. } = command(&[
        "set-activity",
        "--input",
        "a.xer",
        "--dry-run",
        "--activity",
        "A1",
        "--field",
        "activity_name",
        "--value=--draft--",
    ]) else {
        panic!()
    };
    assert_eq!(operation["value"], "--draft--");
    let Command::Edit { operation, .. } = command(&[
        "recalculate-costs",
        "--input",
        "a.xer",
        "--dry-run",
        "--active-only",
        "--sync-overtime",
        "false",
    ]) else {
        panic!()
    };
    assert_eq!(operation["active_only"], true);
    assert_eq!(operation["sync_overtime"], false);
    // usage --costs is a bare true flag like its siblings; unknown flags are
    // refused.
    for (args, want) in [
        (&["--costs"][..], true),
        (&["--costs", "false"][..], false),
        (&[][..], false),
    ] {
        let mut all = vec!["usage", "--input", "a.xer", "--activity", "A1"];
        all.extend_from_slice(args);
        let Command::Usage { costs, .. } = command(&all) else {
            panic!("{all:?}")
        };
        assert_eq!(costs, want, "{all:?}");
    }
    let e = openxer_automation::cli::parse([
        "usage",
        "--input",
        "a.xer",
        "--activity",
        "A1",
        "--value",
        "1",
    ])
    .unwrap_err();
    assert_eq!(e.code, "invalid_arguments", "{e}");
}

/// `--help` piped into a reader that stops early exits cleanly.
#[test]
fn help_into_a_closed_pipe_does_not_panic() {
    use std::process::Stdio;
    let mut child = Process::new(env!("CARGO_BIN_EXE_openxer-cli"))
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(out.status.success(), "{:?} {stderr}", out.status);
}
