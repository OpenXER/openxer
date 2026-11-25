//! Documents for agents: the document edits through `apply`, the command
//! line and MCP, `documents` and `load` listing them, and `read_document`
//! reading the linked files. Every file here is synthetic, written to a
//! temporary folder.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::edit::documents as docs;
use openxer_xer::{starter, Document};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn changes(operations: Value) -> Value {
    json!({"format":"openxer-changes/1","operations":operations})
}

fn first_activity() -> String {
    starter().model.activities[0].task_code.clone()
}

fn first_wbs() -> String {
    let p = starter();
    let id = p
        .wbs
        .iter()
        .find(|w| Some(w.id) != docs_project_node(&p))
        .unwrap()
        .id;
    docs::wbs_path(&p, id)
}

fn docs_project_node(p: &openxer_xer::ProjectData) -> Option<i64> {
    openxer_xer::edit::wbs::project_node(p)
}

/// The starter project written as an XER.
fn source(dir: &Path) -> PathBuf {
    let path = dir.join("source.xer");
    starter().updated_document().write(&path).unwrap();
    path
}

/// A source whose documents point at `files` (reference, location).
fn with_documents(dir: &Path, files: &[(&str, String)]) -> PathBuf {
    let input = source(dir);
    let ops: Vec<Value> = files
        .iter()
        .map(|(reference, location)| {
            json!({"op":"add_document","title":format!("Doc {reference}"),"reference":reference,
                "fields":{"private_location":location}})
        })
        .collect();
    let out = dir.join("with-docs.xer");
    let r =
        run(json!({"command":"apply","input":input,"output":out,"changes":changes(json!(ops))}));
    assert_eq!(r["ok"], true, "{r}");
    out
}

fn read(input: &Path, document: &str, extra: Value) -> Value {
    let mut request = json!({"command":"read_document","input":input,"document":document});
    for (k, v) in extra.as_object().unwrap() {
        request[k] = v.clone();
    }
    run(request)
}

#[test]
fn document_edits_go_through_apply_and_show_in_documents_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let activity = first_activity();
    let wbs = first_wbs();
    let ops = changes(json!([
        {"op":"add_document","title":"Pump datasheet","reference":"DS-100",
            "fields":{"version":"B","author":"Field engineer","date":"2026-03-04",
                "public_location":"C:\\docs\\ds-100.pdf","description":"Rated duty point"}},
        {"op":"add_document","title":"Datasheet markup","parent":"DS-100","reference":"DS-100-M"},
        {"op":"link_document","document":"DS-100","activity":activity,"work_product":true},
        {"op":"link_document","document":"DS-100-M","wbs":wbs},
        {"op":"set_document","document":"ds-100","field":"deliverable","value":true},
        {"op":"set_document","document":"Datasheet markup","field":"version","value":"2"}
    ]));
    // Dry run: the diff, nothing written.
    let dry = run(json!({"command":"apply","input":input,"dry_run":true,"changes":ops}));
    assert_eq!(dry["ok"], true, "{dry}");
    let diff = &dry["result"]["report"]["diff"];
    assert_eq!(
        diff["documents"]["added"].as_array().unwrap().len(),
        2,
        "{diff}"
    );
    assert_eq!(
        diff["document_links"]["added"].as_array().unwrap().len(),
        2,
        "{diff}"
    );
    let out = dir.path().join("out.xer");
    let r = run(json!({"command":"apply","input":input,"output":out,"changes":ops}));
    assert_eq!(r["ok"], true, "{r}");
    let doc = Document::read(&out).unwrap();
    assert_eq!(doc.table("DOCUMENT").unwrap().rows.len(), 2);
    assert_eq!(doc.table("TASKDOC").unwrap().rows.len(), 2);

    let listed = run(json!({"command":"documents","input":out}));
    assert_eq!(listed["ok"], true, "{listed}");
    let list = listed["result"]["documents"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    let ds = &list[0];
    assert_eq!(ds["reference"], "DS-100");
    assert_eq!(ds["version"], "B");
    assert_eq!(ds["date"], "2026-03-04 00:00");
    assert_eq!(ds["deliverable"], true);
    assert_eq!(ds["public_location"], "C:\\docs\\ds-100.pdf");
    assert_eq!(ds["description"], "Rated duty point");
    assert_eq!(ds["activities"][0]["activity"], activity.as_str());
    assert_eq!(ds["activities"][0]["work_product"], true);
    assert_eq!(list[1]["parent"]["reference"], "DS-100");
    assert_eq!(list[1]["wbs"][0]["wbs"], wbs.as_str());
    // Filters.
    let by_activity = run(json!({"command":"documents","input":out,"activity":activity}));
    assert_eq!(by_activity["result"]["count"], 1);
    let by_wbs = run(json!({"command":"documents","input":out,"wbs":wbs}));
    assert_eq!(by_wbs["result"]["documents"][0]["reference"], "DS-100-M");
    let missing = run(json!({"command":"documents","input":out,"activity":"NOPE"}));
    assert_eq!(missing["error"]["code"], "not_found");
    // load lists each activity's documents.
    let loaded = run(json!({"command":"load","input":out}));
    let row = loaded["result"]["activities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["task_code"] == activity.as_str())
        .unwrap()
        .clone();
    assert_eq!(row["documents"][0]["reference"], "DS-100");

    // Unlink, then delete the parent (the child goes with it).
    let next = dir.path().join("next.xer");
    let ops = changes(json!([
        {"op":"unlink_document","document":"DS-100","activity":activity},
        {"op":"delete_document","document":"DS-100"}
    ]));
    let r = run(json!({"command":"apply","input":out,"output":next,"changes":ops}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["report"]["results"][1]["deleted"], 2, "{r}");
    let doc = Document::read(&next).unwrap();
    assert!(doc.table("DOCUMENT").is_none_or(|t| t.rows.is_empty()));
    assert!(doc.table("TASKDOC").is_none_or(|t| t.rows.is_empty()));

    // Failures are all or nothing, with the operation named.
    for (op, why) in [
        (
            json!({"op":"set_document","document":"NOPE","field":"title","value":"x"}),
            "No document",
        ),
        (
            json!({"op":"set_document","document":"DS-100","field":"colour","value":"x"}),
            "Unknown document field",
        ),
        (
            json!({"op":"set_document","document":"DS-100","field":"status","value":"Lost"}),
            "No document status",
        ),
        (
            json!({"op":"link_document","document":"DS-100"}),
            "exactly one of activity or wbs",
        ),
        (
            json!({"op":"unlink_document","document":"DS-100","wbs":wbs}),
            "not linked",
        ),
    ] {
        let r = run(
            json!({"command":"apply","input":out,"dry_run":true,"changes":changes(json!([op]))}),
        );
        assert_eq!(r["ok"], false, "{r}");
        let message = r["error"]["message"].as_str().unwrap();
        assert!(message.contains(why), "{message}");
    }
}

/// A zip with the given entries, stored.
fn zip(path: &Path, entries: &[(&str, &str)]) {
    let file = std::fs::File::create(path).unwrap();
    let mut z = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, text) in entries {
        z.start_file(*name, options).unwrap();
        z.write_all(text.as_bytes()).unwrap();
    }
    z.finish().unwrap();
}

/// A small PDF of `pages`, one line of text each, in a standard font.
fn pdf(path: &Path, pages: &[&str]) {
    let n = pages.len();
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n} >>",
            (0..n)
                .map(|k| format!("{} 0 R", 4 + 2 * k))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    ];
    for (k, text) in pages.iter().enumerate() {
        let content = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * k
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![];
    for (k, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{body}\nendobj\n", k + 1).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    std::fs::write(path, out).unwrap();
}

#[test]
fn read_document_returns_text_docx_and_pdf_pages() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let long: String = (0..500).map(|k| format!("line {k}\n")).collect();
    std::fs::write(d.join("notes.txt"), &long).unwrap();
    std::fs::write(d.join("readme.md"), "\u{feff}# Scope\n\nTie-in at line 12.").unwrap();
    std::fs::write(d.join("list.csv"), "tag,qty\nP-101,2\n").unwrap();
    zip(
        &d.join("spec.docx"),
        &[
            ("[Content_Types].xml", "<Types/>"),
            (
                "word/document.xml",
                r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>Hydrotest at 150 psi</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">Hold &amp; record</w:t><w:tab/><w:t>2 hours</w:t></w:r></w:p></w:body></w:document>"#,
            ),
        ],
    );
    pdf(
        &d.join("drawing.pdf"),
        &["First sheet notes", "Second sheet notes", "Third sheet"],
    );
    let at = |name: &str| d.join(name).to_string_lossy().into_owned();
    let input = with_documents(
        d,
        &[
            ("TXT", at("notes.txt")),
            ("MD", at("readme.md")),
            ("CSV", at("list.csv")),
            ("DOCX", at("spec.docx")),
            ("PDF", at("drawing.pdf")),
        ],
    );

    // Plain text, paged by characters.
    let r = read(&input, "TXT", json!({"limit":20}));
    assert_eq!(r["ok"], true, "{r}");
    let res = &r["result"];
    assert_eq!(res["format"], "text");
    assert_eq!(res["text"], "line 0\nline 1\nline 2");
    assert_eq!(res["next_offset"], 20);
    assert_eq!(res["total_characters"], long.chars().count());
    let r = read(&input, "TXT", json!({"offset":20,"limit":7}));
    assert_eq!(r["result"]["text"], "\nline 3");
    let all = read(&input, "TXT", json!({"limit":200000}));
    assert_eq!(all["result"]["text"], long.as_str());
    assert!(all["result"]["next_offset"].is_null());
    let r = read(&input, "MD", json!({}));
    assert_eq!(r["result"]["text"], "# Scope\n\nTie-in at line 12.");
    let r = read(&input, "CSV", json!({}));
    assert_eq!(r["result"]["text"], "tag,qty\nP-101,2\n");
    // Word body text.
    let r = read(&input, "DOCX", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["format"], "docx");
    assert_eq!(
        r["result"]["text"],
        "Hydrotest at 150 psi\nHold & record\t2 hours\n"
    );
    // PDF page by page.
    let r = read(&input, "PDF", json!({"pages":2}));
    assert_eq!(r["ok"], true, "{r}");
    let res = &r["result"];
    assert_eq!(res["format"], "pdf");
    assert_eq!(res["page_count"], 3);
    assert_eq!(res["pages"].as_array().unwrap().len(), 2);
    assert!(res["pages"][0]["text"]
        .as_str()
        .unwrap()
        .contains("First sheet notes"));
    assert!(res["pages"][1]["text"]
        .as_str()
        .unwrap()
        .contains("Second sheet notes"));
    assert_eq!(res["next_page"], 3);
    let r = read(&input, "PDF", json!({"page":3}));
    assert!(r["result"]["pages"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Third sheet"));
    assert!(r["result"]["next_page"].is_null());
    assert_eq!(
        read(&input, "PDF", json!({"page":9}))["error"]["code"],
        "invalid_arguments"
    );
    assert_eq!(
        read(&input, "PDF", json!({"offset":3}))["error"]["code"],
        "invalid_arguments"
    );
    assert_eq!(
        read(&input, "TXT", json!({"page":1}))["error"]["code"],
        "invalid_arguments"
    );
    assert_eq!(
        read(&input, "TXT", json!({"limit":0}))["error"]["code"],
        "invalid_arguments"
    );
    assert_eq!(
        read(&input, "TXT", json!({"offset":999999}))["error"]["code"],
        "invalid_arguments"
    );
}

#[test]
fn read_document_refuses_programs_paths_it_does_not_trust_and_unknown_types() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("tool.exe"), "MZ").unwrap();
    std::fs::write(d.join("run.ps1"), "Write-Host hi").unwrap();
    std::fs::write(d.join("pack.zip"), "PK").unwrap();
    std::fs::write(d.join("broken.pdf"), "not a pdf").unwrap();
    std::fs::write(d.join("broken.docx"), "not a zip").unwrap();
    let at = |name: &str| d.join(name).to_string_lossy().into_owned();
    let input = with_documents(
        d,
        &[
            ("EXE", at("tool.exe")),
            ("PS1", at("run.ps1")),
            ("ZIP", at("pack.zip")),
            ("REL", "notes.txt".into()),
            ("UNC", "\\\\server\\share\\notes.txt".into()),
            ("URL", "https://example.com/notes.txt".into()),
            ("GONE", at("missing.txt")),
            ("BADPDF", at("broken.pdf")),
            ("BADDOCX", at("broken.docx")),
            ("NONE", String::new()),
        ],
    );
    for (document, code) in [
        ("EXE", "refused_type"),
        ("PS1", "refused_type"),
        ("ZIP", "unsupported_type"),
        ("REL", "location_refused"),
        ("UNC", "location_refused"),
        ("URL", "location_refused"),
        ("GONE", "location_refused"),
        ("BADPDF", "read_failed"),
        ("BADDOCX", "read_failed"),
        ("NONE", "no_location"),
        ("NOT-A-DOCUMENT", "document_not_found"),
    ] {
        let r = read(&input, document, json!({}));
        assert_eq!(r["ok"], false, "{document}: {r}");
        assert_eq!(r["error"]["code"], code, "{document}: {r}");
    }
    let r = read(&input, "UNC", json!({}));
    assert!(r["error"]["message"].as_str().unwrap().contains("network"));
    let r = read(&input, "EXE", json!({"location":"public"}));
    assert_eq!(r["error"]["code"], "no_location");
    let r = read(&input, "EXE", json!({"location":"elsewhere"}));
    assert_eq!(r["error"]["code"], "invalid_arguments");
}

#[test]
fn the_command_line_and_mcp_reach_the_same_documents() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("notes.txt"), "Crane lift plan").unwrap();
    let input = with_documents(
        d,
        &[("LIFT", d.join("notes.txt").to_string_lossy().into_owned())],
    );
    let activity = first_activity();
    let cli_run = |args: &[&str]| -> Value {
        match cli::parse(args.iter().map(|s| s.to_string())).unwrap() {
            Invocation::Run(c) => response(dispatch(*c)),
            _ => panic!("{args:?}"),
        }
    };
    let input_s = input.to_str().unwrap();
    let r = cli_run(&["documents", "--input", input_s]);
    assert_eq!(r["result"]["documents"][0]["reference"], "LIFT");
    let r = cli_run(&[
        "read-document",
        "--input",
        input_s,
        "--document",
        "LIFT",
        "--limit",
        "5",
    ]);
    assert_eq!(r["result"]["text"], "Crane", "{r}");
    assert!(cli::parse(["read-document", "--input", input_s]).is_err());
    let out = d.join("linked.xer");
    let r = cli_run(&[
        "link-document",
        "--input",
        input_s,
        "--document",
        "LIFT",
        "--activity",
        &activity,
        "--work-product",
        "true",
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(r["ok"], true, "{r}");
    let r = cli_run(&[
        "documents",
        "--input",
        out.to_str().unwrap(),
        "--activity",
        &activity,
    ]);
    assert_eq!(
        r["result"]["documents"][0]["activities"][0]["work_product"],
        true
    );
    let r = cli_run(&[
        "set-document",
        "--input",
        input_s,
        "--document",
        "LIFT",
        "--field",
        "version",
        "--value",
        "3",
        "--dry-run",
        "true",
    ]);
    assert_eq!(r["ok"], true, "{r}");
    for flag in [
        "add-document",
        "set-document",
        "delete-document",
        "link-document",
        "unlink-document",
        "documents",
        "read-document",
        "add_document",
        "link_document",
    ] {
        assert!(cli::HELP.contains(flag), "{flag} in --help");
    }

    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let list = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":meta}}))
        .unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for tool in [
        "openxer_documents",
        "openxer_read_document",
        "openxer_add_document",
        "openxer_set_document",
        "openxer_delete_document",
        "openxer_link_document",
        "openxer_unlink_document",
    ] {
        assert!(names.contains(&tool), "{tool}");
    }
    let call = |server: &mut Server, name: &str, args: Value| -> Value {
        server
            .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
                "params":{"_meta":meta,"name":name,"arguments":args}}))
            .unwrap()
    };
    let r = call(
        &mut server,
        "openxer_read_document",
        json!({"input":input,"document":"LIFT"}),
    );
    assert_eq!(
        r["result"]["structuredContent"]["result"]["text"], "Crane lift plan",
        "{r}"
    );
    let r = call(&mut server, "openxer_read_document", json!({"input":input}));
    assert!(r["error"].is_object(), "document is required: {r}");
    let r = call(
        &mut server,
        "openxer_read_document",
        json!({"input":input,"document":"LIFT","path":"C:\\x.txt"}),
    );
    assert!(r["error"].is_object(), "no free path: {r}");
    let mcp_out = d.join("mcp.xer");
    let r = call(
        &mut server,
        "openxer_link_document",
        json!({"input":input,"output":mcp_out,
        "document":"LIFT","activity":activity,"work_product":true}),
    );
    assert_eq!(r["result"]["structuredContent"]["ok"], true, "{r}");
    let rows = |p: &Path| {
        Document::read(p)
            .unwrap()
            .table("TASKDOC")
            .unwrap()
            .rows
            .clone()
    };
    assert_eq!(rows(&mcp_out), rows(&out));
    let r = call(
        &mut server,
        "openxer_documents",
        json!({"input":mcp_out,"activity":activity}),
    );
    assert_eq!(
        r["result"]["structuredContent"]["result"]["count"], 1,
        "{r}"
    );
    let r = call(
        &mut server,
        "openxer_add_document",
        json!({"input":input,"dry_run":true,
        "title":"Rigging study","fields":{"status":null,"author":"Lift engineer"}}),
    );
    assert_eq!(r["result"]["structuredContent"]["ok"], true, "{r}");
}

/// A PDF page cut by `limit` says so, and `next_page` comes back to it.
#[test]
fn a_pdf_page_cut_by_the_limit_is_marked_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    pdf(
        &d.join("drawing.pdf"),
        &["First sheet notes", "Second sheet notes"],
    );
    let input = with_documents(
        d,
        &[("PDF", d.join("drawing.pdf").to_string_lossy().into_owned())],
    );
    let r = read(&input, "PDF", json!({"limit":5}));
    assert_eq!(r["ok"], true, "{r}");
    let res = &r["result"];
    assert_eq!(res["truncated"], true, "{res}");
    assert_eq!(res["pages"].as_array().unwrap().len(), 1, "{res}");
    assert_eq!(res["pages"][0]["truncated"], true, "{res}");
    assert!(res["pages"][0]["page_characters"].as_u64().unwrap() > 5);
    assert_eq!(res["next_page"], 1, "the rest of page 1 is still to read");
    let r = read(&input, "PDF", json!({}));
    assert_eq!(r["result"]["truncated"], false, "{r}");
    assert!(r["result"]["pages"][0].get("truncated").is_none());
}

/// A document row can name any local file; only files under the input's
/// own folder are read unless OPENXER_DOCUMENT_ROOTS allows another folder.
#[test]
fn read_document_reads_only_under_the_input_folder_unless_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::write(elsewhere.path().join("secret.json"), "{\"token\":\"x\"}").unwrap();
    std::fs::write(dir.path().join("notes.txt"), "Tie-in at line 12").unwrap();
    let input = with_documents(
        dir.path(),
        &[
            (
                "OUT",
                elsewhere
                    .path()
                    .join("secret.json")
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "IN",
                dir.path().join("notes.txt").to_string_lossy().into_owned(),
            ),
        ],
    );
    let r = read(&input, "IN", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    let r = read(&input, "OUT", json!({}));
    assert_eq!(r["error"]["code"], "location_refused", "{r}");
    let message = r["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("outside the input file's folder"),
        "{message}"
    );
    assert!(message.contains("OPENXER_DOCUMENT_ROOTS"), "{message}");
    assert!(!message.contains("token"), "{message}");
    // The opt-in, given to the process that serves the request.
    let exe = env!("CARGO_BIN_EXE_openxer-cli");
    let run_with = |roots: Option<&Path>| -> Value {
        let mut command = std::process::Command::new(exe);
        command
            .args(["read-document", "--input"])
            .arg(&input)
            .args(["--document", "OUT"]);
        command.env_remove("OPENXER_DOCUMENT_ROOTS");
        if let Some(root) = roots {
            command.env("OPENXER_DOCUMENT_ROOTS", root);
        }
        serde_json::from_slice(&command.output().unwrap().stdout).unwrap()
    };
    assert_eq!(run_with(None)["error"]["code"], "location_refused");
    let allowed = run_with(Some(elsewhere.path()));
    assert_eq!(allowed["ok"], true, "{allowed}");
    assert_eq!(allowed["result"]["text"], "{\"token\":\"x\"}");
}
