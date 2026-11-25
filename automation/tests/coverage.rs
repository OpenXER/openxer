//! Every edit operation and every MCP tool is documented where agents and
//! users look: `--help`, the `openxer_apply` tool description,
//! `automation/README.md` and the coverage map `automation/COVERAGE.md`.
//! The desktop's guard (`desktop/src/agent_coverage.rs`) checks the other
//! side: every command the app draws maps to these.
use openxer_automation::{cli::HELP, edit, mcp::tool_definitions};

/// The operation names, from the edit format's own list of variants.
fn operations() -> Vec<String> {
    let error = edit::operation(serde_json::json!({"op":"__none__"})).unwrap_err();
    let list = error
        .split("expected one of ")
        .nth(1)
        .unwrap_or_else(|| panic!("variant list in {error:?}"));
    list.split(", ")
        .map(|s| s.trim().trim_matches('`').to_string())
        .collect()
}

fn read(name: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name))
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn every_operation_is_documented_in_help_mcp_readme_and_coverage() {
    let ops = operations();
    assert!(ops.len() > 60, "{ops:?}");
    let mut listed: Vec<String> = edit::OPERATIONS.iter().map(|s| s.to_string()).collect();
    let mut known = ops.clone();
    listed.sort();
    known.sort();
    assert_eq!(listed, known, "edit::OPERATIONS lists every operation once");
    assert_eq!(ops.iter().filter(|o| *o == "set_activity").count(), 1);
    let apply = tool_definitions()
        .into_iter()
        .find(|t| t["name"] == "openxer_apply")
        .unwrap()["description"]
        .as_str()
        .unwrap()
        .to_string();
    let readme = read("README.md");
    let coverage = read("COVERAGE.md");
    let help = HELP.replace('\n', " ");
    let mut missing = vec![];
    for op in &ops {
        let quoted = format!("`{op}`");
        for (place, text, needle) in [
            ("--help", help.as_str(), op.as_str()),
            ("openxer_apply", apply.as_str(), op.as_str()),
            ("README.md", readme.as_str(), quoted.as_str()),
            ("COVERAGE.md", coverage.as_str(), quoted.as_str()),
        ] {
            let found = text.match_indices(needle).any(|(i, _)| {
                let before = text[..i].chars().next_back();
                let after = text[i + needle.len()..].chars().next();
                let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
                !word(before) && !word(after)
            });
            if !found {
                missing.push(format!("{op} in {place}"));
            }
        }
    }
    assert!(missing.is_empty(), "undocumented: {missing:#?}");
}

#[test]
fn every_mcp_tool_is_in_the_readme_and_the_coverage_map() {
    let readme = read("README.md");
    let coverage = read("COVERAGE.md");
    let mut missing = vec![];
    for tool in tool_definitions() {
        let name = tool["name"].as_str().unwrap();
        let quoted = format!("`{name}`");
        if !readme.contains(&quoted) {
            missing.push(format!("{name} in README.md"));
        }
        if !coverage.contains(&quoted) {
            missing.push(format!("{name} in COVERAGE.md"));
        }
    }
    assert!(missing.is_empty(), "undocumented: {missing:#?}");
}
