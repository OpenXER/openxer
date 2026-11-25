use openxer_automation::{
    cli::{self, Invocation},
    dispatch, Command,
};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command as Process;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("openxer-auto-ix-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn starter_xer(d: &std::path::Path) -> PathBuf {
    let path = d.join("starter.xer");
    openxer_xer::starter()
        .updated_document()
        .write(&path)
        .unwrap();
    path
}

fn run(args: &[&str]) -> Value {
    match cli::parse(args.iter().copied()).unwrap() {
        Invocation::Run(c) => dispatch(*c).unwrap_or_else(|e| panic!("{e}")),
        other => panic!("expected a command, got {other:?}"),
    }
}

#[test]
fn cli_parses_new_commands_and_flags() {
    let Invocation::Run(c) = cli::parse([
        "import-xlsx",
        "--input",
        "a.db",
        "--spreadsheet",
        "b.xlsx",
        "--dry-run",
    ])
    .unwrap() else {
        panic!()
    };
    assert!(matches!(
        *c,
        Command::ImportXlsx {
            dry_run: true,
            output: None,
            ..
        }
    ));
    let Invocation::Run(c) = cli::parse([
        "export-xlsx",
        "--input",
        "a.xer",
        "--output",
        "b.xlsx",
        "--layout",
        "tables",
        "--unit",
        "hours",
    ])
    .unwrap() else {
        panic!()
    };
    assert!(matches!(*c, Command::ExportXlsx { .. }));
    let Invocation::Run(c) = cli::parse([
        "import",
        "--input",
        "a.xer",
        "--output",
        "b.db",
        "--workspace",
        "w.db",
        "--action",
        "update",
        "--target-workspace-id",
        "3",
        "--config",
        "activities=update+delete",
    ])
    .unwrap() else {
        panic!()
    };
    assert!(matches!(
        *c,
        Command::Import {
            target_workspace_id: Some(3),
            ..
        }
    ));
    assert!(cli::parse(["export-xlsx", "--input", "a", "--bogus", "x"]).is_err());
}

#[test]
fn export_xlsx_then_dry_run_and_apply_import() {
    let d = dir("xlsx");
    let xer = starter_xer(&d);
    let modern = d.join("modern.xlsx");
    let r = run(&[
        "export-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        modern.to_str().unwrap(),
        "--columns",
        "activity_id,activity_name,remaining_duration,start,finish",
    ]);
    assert_eq!(r["activities"], 14);
    assert!(modern.exists());
    // Existing destinations are refused.
    let again = cli::parse([
        "export-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        modern.to_str().unwrap(),
    ])
    .unwrap();
    let Invocation::Run(c) = again else { panic!() };
    assert_eq!(dispatch(*c).unwrap_err().code, "output_exists");
    // Unedited: the dry run reports no changes.
    let r = run(&[
        "import-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--spreadsheet",
        modern.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(r["counts"]["changed"], 0);
    assert_eq!(r["counts"]["unchanged"], 14);
    assert_eq!(r["rows"].as_array().unwrap().len(), 0);
    // A plain tracker sheet with one rename and one bad row.
    let plain = d.join("tracker.xlsx");
    let mut book = rust_xlsxwriter::Workbook::new();
    let ws = book.add_worksheet();
    for (c, h) in ["Activity ID", "Activity Name", "Remaining Duration (h)"]
        .iter()
        .enumerate()
    {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    ws.write_string(1, 0, "RB-1010").unwrap();
    ws.write_string(1, 1, "Survey (CLI)").unwrap();
    ws.write_number(1, 2, 16).unwrap();
    ws.write_string(2, 0, "RB-1020").unwrap();
    ws.write_string(2, 2, "soon").unwrap();
    book.save(&plain).unwrap();
    let r = run(&[
        "import-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--spreadsheet",
        plain.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(r["counts"]["changed"], 1);
    assert_eq!(r["counts"]["errors"], 1);
    let rows = r["rows"].as_array().unwrap();
    assert!(rows
        .iter()
        .any(|row| row["changes"][0]["new"] == "Survey (CLI)"));
    assert!(!d.join("applied.xer").exists());
    let out = d.join("applied.xer");
    let r = run(&[
        "import-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--spreadsheet",
        plain.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(r["applied"]["changed"], 1);
    let p = openxer_xer::ProjectData::from_document(
        openxer_xer::Document::read(&out).unwrap(),
        1,
        "x".into(),
    )
    .unwrap();
    assert_eq!(p.details[&1010].name, "Survey (CLI)");
    assert_eq!(p.model.activities[1].rem, 16 * 60);
    // Tables layout.
    let tables = d.join("tables.xlsx");
    let r = run(&[
        "export-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        tables.to_str().unwrap(),
        "--layout",
        "tables",
    ]);
    assert_eq!(r["layout"], "tables");
    // The old layout name still works from old scripts and reports the new one.
    let old = d.join("old-name.xlsx");
    let r = run(&[
        "export-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        old.to_str().unwrap(),
        "--layout",
        "p6",
    ]);
    assert_eq!(r["layout"], "tables");
    let r = run(&[
        "import-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--spreadsheet",
        tables.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(r["format"], "Tables");
    assert_eq!(r["counts"]["changed"], 0);
}

#[test]
fn xer_import_into_workspace_and_versioned_export() {
    let d = dir("xer");
    let xer = starter_xer(&d);
    let ws = d.join("ws.db");
    run(&[
        "import",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        ws.to_str().unwrap(),
    ]);
    // A contractor file with one renamed activity.
    let mut edited = openxer_xer::starter();
    edited.details.get_mut(&1030).unwrap().name = "Package issued (contractor)".into();
    let update = d.join("update.xer");
    edited.updated_document().write(&update).unwrap();
    let ws2 = d.join("ws2.db");
    let r = run(&[
        "import",
        "--input",
        update.to_str().unwrap(),
        "--output",
        ws2.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--action",
        "update",
        "--config",
        "relationships=update+delete",
    ]);
    assert_eq!(
        r["projects"][0]["report"]["counts"]["activities"]["updated"],
        1
    );
    let loaded = run(&["load", "--input", ws2.to_str().unwrap(), "--project", "1"]);
    assert!(loaded["activities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["name"] == "Package issued (contractor)"));
    // The source workspace is untouched.
    let original = run(&["load", "--input", ws.to_str().unwrap(), "--project", "1"]);
    assert!(!original["activities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["name"] == "Package issued (contractor)"));
    // Create New adds a second row; Replace swaps content in place.
    let ws3 = d.join("ws3.db");
    let r = run(&[
        "import",
        "--input",
        update.to_str().unwrap(),
        "--output",
        ws3.to_str().unwrap(),
        "--workspace",
        ws2.to_str().unwrap(),
        "--action",
        "new",
    ]);
    assert_eq!(r["projects"][0]["action"], "new");
    let inspect = run(&["inspect", "--input", ws3.to_str().unwrap()]);
    assert_eq!(inspect["projects"].as_array().unwrap().len(), 2);
    // Versioned export of one workspace project.
    let out = d.join("v19.xer");
    let r = run(&[
        "export",
        "--input",
        ws2.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--version",
        "19.12",
        "--project",
        "1",
    ]);
    assert_eq!(r["version"], "19.12");
    let doc = openxer_xer::Document::read(&out).unwrap();
    assert!(doc.header.starts_with("ERMHDR\t19.12\t"));
    // Bad config is rejected before anything is written.
    let bad = d.join("bad.db");
    let parsed = cli::parse([
        "import",
        "--input",
        update.to_str().unwrap(),
        "--output",
        bad.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--action",
        "update",
        "--config",
        "wbs=insert",
    ])
    .unwrap();
    let Invocation::Run(c) = parsed else { panic!() };
    assert_eq!(dispatch(*c).unwrap_err().code, "invalid_arguments");
    assert!(!bad.exists());
}

#[test]
fn cli_binary_prints_dry_run_diff_as_json() {
    let d = dir("bin");
    let xer = starter_xer(&d);
    let sheet = d.join("s.xlsx");
    run(&[
        "export-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        sheet.to_str().unwrap(),
    ]);
    let out = Process::new(env!("CARGO_BIN_EXE_openxer-cli"))
        .args([
            "import-xlsx",
            "--input",
            xer.to_str().unwrap(),
            "--spreadsheet",
            sheet.to_str().unwrap(),
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["result"]["dry_run"], true);
    assert_eq!(v["result"]["format"], "OpenXer");
}

#[test]
fn export_xlsx_runs_a_saved_template_by_name_or_from_a_file() {
    use openxer_interchange::columns::Column;
    use openxer_interchange::export_template::{
        list_json, Condition, ExportTemplate, Op, SortKey, PREFERENCE_KEY,
    };
    use openxer_interchange::xlsx_import::read_workbook;
    let d = dir("template");
    let xer = starter_xer(&d);
    let db = d.join("workspace.db");
    run(&[
        "import",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        db.to_str().unwrap(),
    ]);
    let mut t = ExportTemplate::new(
        "Open work",
        &[
            Column::ActivityId,
            Column::ActivityName,
            Column::RemainingDuration,
        ],
    );
    t.group_by_wbs = false;
    t.filter = vec![Condition {
        column: Column::RemainingDuration.key(),
        op: Op::GreaterThan,
        value: "0".into(),
    }];
    t.sort = vec![SortKey {
        column: Column::ActivityId.key(),
        descending: true,
    }];
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "INSERT INTO preferences(key,value) VALUES(?1,?2)",
        [PREFERENCE_KEY, &list_json(std::slice::from_ref(&t))],
    )
    .unwrap();
    drop(conn);
    let rows = |path: &std::path::Path| -> Vec<String> {
        let wb = read_workbook(path).unwrap();
        let s = wb.sheet("Activities").unwrap();
        wb.sheets[s]
            .rows
            .iter()
            .skip(1)
            .map(|r| r[0].text())
            .collect()
    };
    let by_name = d.join("by-name.xlsx");
    let r = run(&[
        "export-xlsx",
        "--input",
        db.to_str().unwrap(),
        "--output",
        by_name.to_str().unwrap(),
        "--template",
        "open WORK",
    ]);
    let ids = rows(&by_name);
    assert_eq!(r["activities"], ids.len());
    assert!(
        !ids.is_empty() && ids.len() < 14,
        "milestones are filtered out"
    );
    assert!(
        ids.windows(2).all(|w| w[0] > w[1]),
        "sorted descending: {ids:?}"
    );
    // The same template from a file gives the same rows.
    let file = d.join("open-work.json");
    std::fs::write(&file, serde_json::to_string(&t).unwrap()).unwrap();
    let by_file = d.join("by-file.xlsx");
    run(&[
        "export-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        by_file.to_str().unwrap(),
        "--template",
        file.to_str().unwrap(),
    ]);
    assert_eq!(rows(&by_file), ids);
    // An unknown name lists the saved ones; a template excludes --columns.
    let unknown = cli::parse([
        "export-xlsx",
        "--input",
        db.to_str().unwrap(),
        "--output",
        d.join("none.xlsx").to_str().unwrap(),
        "--template",
        "nothing",
    ])
    .unwrap();
    let Invocation::Run(c) = unknown else {
        panic!()
    };
    let e = dispatch(*c).unwrap_err();
    assert_eq!(e.code, "template_not_found");
    assert!(e.message.contains("Open work"), "{}", e.message);
    let both = cli::parse([
        "export-xlsx",
        "--input",
        db.to_str().unwrap(),
        "--output",
        d.join("both.xlsx").to_str().unwrap(),
        "--template",
        "Open work",
        "--columns",
        "activity_id",
    ])
    .unwrap();
    let Invocation::Run(c) = both else { panic!() };
    assert_eq!(dispatch(*c).unwrap_err().code, "invalid_arguments");
    // A name needs a workspace; a malformed file is refused.
    let no_workspace = cli::parse([
        "export-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        d.join("nw.xlsx").to_str().unwrap(),
        "--template",
        "Open work",
    ])
    .unwrap();
    let Invocation::Run(c) = no_workspace else {
        panic!()
    };
    assert_eq!(dispatch(*c).unwrap_err().code, "invalid_arguments");
    std::fs::write(&file, r#"{"name":"x","subject":"Activities"}"#).unwrap();
    let bad = cli::parse([
        "export-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        d.join("bad.xlsx").to_str().unwrap(),
        "--template",
        file.to_str().unwrap(),
    ])
    .unwrap();
    let Invocation::Run(c) = bad else { panic!() };
    assert!(dispatch(*c).is_err());
}

#[test]
fn mcp_export_xlsx_publishes_tables_and_accepts_the_old_layout_name() {
    let d = dir("mcp-layout");
    let xer = starter_xer(&d);
    let mut server = openxer_automation::mcp::Server::default();
    let meta = serde_json::json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let list = server
        .handle(
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list",
            "params":{"_meta":meta}}),
        )
        .unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    let export = tools
        .iter()
        .find(|t| t["name"] == "openxer_export_xlsx")
        .unwrap();
    let layouts = &export["inputSchema"]["properties"]["layout"]["enum"];
    assert_eq!(layouts, &serde_json::json!(["modern", "tables"]));
    for (n, layout) in ["tables", "p6"].into_iter().enumerate() {
        let out = d.join(format!("mcp-{n}.xlsx"));
        let reply = server
            .handle(
                serde_json::json!({"jsonrpc":"2.0","id":n + 2,"method":"tools/call",
                "params":{"_meta":meta,"name":"openxer_export_xlsx",
                    "arguments":{"input":xer,"output":out,"layout":layout}}}),
            )
            .unwrap();
        let result = &reply["result"]["structuredContent"];
        assert_eq!(result["ok"], true, "{layout}: {reply}");
        assert_eq!(result["result"]["layout"], "tables", "{layout}: {reply}");
    }
}

/// Import into a workspace keeps the file name (not the local path) and
/// names a bad action as such; a version-only export keeps every project
/// of the file like the plain export; import-xlsx refuses an output with
/// a dry run; an unknown --projects id says what was looked for.
#[test]
fn import_and_export_options_behave_like_their_plain_forms() {
    let d = dir("options");
    let xer = starter_xer(&d);
    let ws = d.join("ws.db");
    run(&[
        "import",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        ws.to_str().unwrap(),
    ]);
    let ws2 = d.join("ws2.db");
    run(&[
        "import",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        ws2.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--action",
        "new",
    ]);
    let inspect = run(&["inspect", "--input", ws2.to_str().unwrap()]);
    for row in inspect["projects"].as_array().unwrap() {
        let source = row["source_name"].as_str().unwrap();
        assert!(source.starts_with("starter.xer"), "{source}");
        assert!(!source.contains(['\\', '/']), "{source}");
    }
    // A bad action on a file of several projects is named.
    let portfolio = d.join("portfolio.xer");
    openxer_xer::starter_portfolio().write(&portfolio).unwrap();
    let bad = d.join("bad.db");
    let parsed = cli::parse([
        "import",
        "--input",
        portfolio.to_str().unwrap(),
        "--output",
        bad.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--action",
        "merge",
    ])
    .unwrap();
    let Invocation::Run(c) = parsed else { panic!() };
    let e = dispatch(*c).unwrap_err();
    assert!(e.message.contains("action must be"), "{e}");
    // --version alone keeps every project of the file.
    let doc = openxer_xer::Document::read(&portfolio).unwrap();
    let projects = doc.projects();
    assert!(projects.len() > 1);
    let out = d.join("versioned.xer");
    let r = run(&[
        "export",
        "--input",
        portfolio.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--version",
        "19.12",
        "--project",
        &projects[0].0.to_string(),
    ]);
    assert_eq!(r["version"], "19.12");
    let written = openxer_xer::Document::read(&out).unwrap();
    assert!(written.header.starts_with("ERMHDR\t19.12\t"));
    assert_eq!(written.projects().len(), projects.len());
    // An id that is neither a workspace row nor a project.
    let none = d.join("none.xer");
    let parsed = cli::parse([
        "export",
        "--input",
        ws.to_str().unwrap(),
        "--output",
        none.to_str().unwrap(),
        "--projects",
        "987654",
    ])
    .unwrap();
    let Invocation::Run(c) = parsed else { panic!() };
    let e = dispatch(*c).unwrap_err();
    assert_eq!(e.code, "project_not_found");
    assert!(e.message.contains("987654"), "{e}");
    // import-xlsx: --output with --dry-run is refused like the edits.
    let parsed = cli::parse([
        "import-xlsx",
        "--input",
        xer.to_str().unwrap(),
        "--spreadsheet",
        d.join("missing.xlsx").to_str().unwrap(),
        "--output",
        d.join("x.xer").to_str().unwrap(),
        "--dry-run",
    ])
    .unwrap();
    let Invocation::Run(c) = parsed else { panic!() };
    let e = dispatch(*c).unwrap_err();
    assert_eq!(e.code, "invalid_arguments");
    assert!(e.message.contains("omit output"), "{e}");
}

/// Export writes project XML by format or by an .xml output name; import
/// and load read it back like the XER it came from, and the MCP export
/// takes the same format argument.
#[test]
fn export_and_import_project_xml() {
    let d = dir("xml");
    let xer = starter_xer(&d);
    let ws = d.join("ws.db");
    run(&[
        "import",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        ws.to_str().unwrap(),
    ]);
    let by_name = d.join("plain.xml");
    let r = run(&[
        "export",
        "--input",
        ws.to_str().unwrap(),
        "--project",
        "1",
        "--output",
        by_name.to_str().unwrap(),
    ]);
    assert_eq!(r["format"], "xml");
    let text = std::fs::read_to_string(&by_name).unwrap();
    let root = format!("<{} xmlns=", openxer_interchange::project_xml::ROOT);
    assert!(text.contains(&root), "{text}");
    let by_format = d.join("chosen.xml");
    let r = run(&[
        "export",
        "--input",
        xer.to_str().unwrap(),
        "--projects",
        "1",
        "--format",
        "xml",
        "--output",
        by_format.to_str().unwrap(),
    ]);
    assert_eq!(r["format"], "xml");
    let inspect = run(&["inspect", "--input", by_format.to_str().unwrap()]);
    assert_eq!(inspect["format"], "xml");
    // Nothing in a file OpenXER wrote is skipped, and the project's own WBS
    // node is not counted as a WBS element.
    let report = &inspect["xml"];
    assert_eq!(report["unhandled"], serde_json::json!({}), "{inspect}");
    let tables = inspect["tables"].as_array().unwrap();
    let wbs = tables.iter().find(|t| t["name"] == "PROJWBS").unwrap();
    let elements = report["elements"]["WBS"].as_u64().unwrap_or(0);
    assert_eq!(wbs["row_count"].as_u64(), Some(elements));
    assert_eq!(wbs["project_nodes"], 1);
    // Import the XML into a new workspace: the same activities and links.
    let from_xml = d.join("from-xml.db");
    run(&[
        "import",
        "--input",
        by_name.to_str().unwrap(),
        "--output",
        from_xml.to_str().unwrap(),
        "--format",
        "xml",
    ]);
    let names = |path: &PathBuf| {
        let loaded = run(&["load", "--input", path.to_str().unwrap(), "--project", "1"]);
        let mut acts: Vec<String> = loaded["activities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| format!("{} {}", a["task_code"], a["name"]))
            .collect();
        acts.sort();
        (acts, loaded["relationships"].as_array().unwrap().len())
    };
    assert_eq!(names(&from_xml), names(&ws));
    // A format that does not match the file is refused before writing.
    let wrong = d.join("wrong.db");
    let parsed = cli::parse([
        "import",
        "--input",
        xer.to_str().unwrap(),
        "--output",
        wrong.to_str().unwrap(),
        "--format",
        "xml",
    ])
    .unwrap();
    let Invocation::Run(c) = parsed else { panic!() };
    assert_eq!(dispatch(*c).unwrap_err().code, "invalid_input");
    assert!(!wrong.exists());
    // MCP: the export tool lists the format and writes XML with it.
    let mut server = openxer_automation::mcp::Server::default();
    let meta = serde_json::json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let list = server
        .handle(
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list",
            "params":{"_meta":meta}}),
        )
        .unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    for name in ["openxer_export", "openxer_import"] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(
            tool["inputSchema"]["properties"]["format"]["enum"],
            serde_json::json!(["xer", "xml"]),
            "{name}"
        );
    }
    let out = d.join("mcp.xml");
    let reply = server
        .handle(
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_export",
                "arguments":{"input":xer,"output":out,"format":"xml"}}}),
        )
        .unwrap();
    let result = &reply["result"]["structuredContent"];
    assert_eq!(result["ok"], true, "{reply}");
    assert!(std::fs::read_to_string(&out).unwrap().contains("<Project>"));
}
