//! export-pdf: argument parsing, the MCP tool, output checks, and an end to
//! end run through the desktop renderer when its executable is present.
use openxer_automation::{cli, dispatch, mcp, pdf, Command};
use std::path::PathBuf;

fn temp(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "openxer-cli-pdf-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn export_pdf_flags_parse_into_the_shared_command() {
    let parsed = cli::parse([
        "export-pdf",
        "--input",
        "plan.xer",
        "--output",
        "plan.pdf",
        "--layout",
        "Look Ahead",
        "--view",
        "network",
        "--paper",
        "A3",
        "--orientation",
        "portrait",
        "--project",
        "7",
    ])
    .unwrap();
    let cli::Invocation::Run(command) = parsed else {
        panic!("a command");
    };
    match *command {
        Command::ExportPdf {
            input,
            output,
            project_id,
            layout,
            view,
            paper,
            orientation,
            ..
        } => {
            assert_eq!(input, PathBuf::from("plan.xer"));
            assert_eq!(output, PathBuf::from("plan.pdf"));
            assert_eq!(project_id, Some(7));
            assert_eq!(layout.as_deref(), Some("Look Ahead"));
            assert_eq!(view.as_deref(), Some("network"));
            assert_eq!(paper.as_deref(), Some("A3"));
            assert_eq!(orientation.as_deref(), Some("portrait"));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(cli::parse(["export-pdf", "--input", "a.xer", "--bogus", "1"]).is_err());
    let tools = mcp::tool_definitions();
    let tool = tools
        .iter()
        .find(|t| t["name"] == "openxer_export_pdf")
        .expect("MCP lists export_pdf");
    assert_eq!(
        tool["inputSchema"]["required"],
        serde_json::json!(["input", "output"])
    );
}

#[test]
fn export_pdf_writes_a_pdf_through_the_desktop_renderer() {
    let dir = temp("e2e");
    let xer = dir.join("starter.xer");
    std::fs::write(&xer, openxer_xer::starter().updated_document().text()).unwrap();
    let bad = dispatch(Command::ExportPdf {
        input: xer.clone(),
        output: dir.join("out.txt"),
        project_id: None,
        workspace_id: None,
        layout: None,
        view: None,
        paper: None,
        orientation: None,
    });
    assert_eq!(bad.unwrap_err().code, "invalid_arguments");
    // Only drive a real desktop executable when one is named explicitly: a
    // `target` folder can hold a stale build from before `export-pdf` existed,
    // which would open its window instead of printing.
    if std::env::var_os("OPENXER_DESKTOP").is_none() {
        eprintln!("SKIPPED desktop render: set OPENXER_DESKTOP to a current OpenXER build");
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    let output = dir.join("starter.pdf");
    let result = dispatch(Command::ExportPdf {
        input: xer,
        output: output.clone(),
        project_id: None,
        workspace_id: None,
        layout: None,
        view: Some("gantt".into()),
        paper: Some("Letter".into()),
        orientation: Some("landscape".into()),
    });
    match pdf::desktop_binary() {
        Some(_) => {
            let summary = result.unwrap();
            assert_eq!(summary["pages"], 1);
            assert_eq!(summary["paper"], "Letter");
            let bytes = std::fs::read(&output).unwrap();
            assert!(bytes.starts_with(b"%PDF-"));
        }
        // `cargo test -p openxer-automation` alone does not build the desktop.
        None => assert_eq!(result.unwrap_err().code, "desktop_missing"),
    }
    let _ = std::fs::remove_dir_all(dir);
}
