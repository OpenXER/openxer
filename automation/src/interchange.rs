//! Spreadsheet export/import and XER import actions for the shared dispatcher.
//! Outputs follow the crate rule: always a new file, published without overwriting.
use crate::storage::{self, invalid, io, parent, publish};
use crate::{project_json, AutomationError, Result};
use openxer_interchange::columns::{Column, DurationUnit, Lookup};
use openxer_interchange::export_template::{self, ExportTemplate};
use openxer_interchange::xer_export::export_document;
use openxer_interchange::xer_merge::{merge_project, ImportConfig};
use openxer_interchange::xlsx_export::{modern_workbook, tables_workbook, ExportOptions};
use openxer_interchange::xlsx_import as xi;
use openxer_xer::{Document, ProjectData};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;

fn args(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

pub(crate) fn check_new_output(path: &Path, extensions: &[&str]) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(args("output path cannot be empty"));
    }
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(AutomationError::new(
            "output_exists",
            format!("Destination already exists: {}", path.display()),
        ));
    }
    let ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)));
    if !ok {
        return Err(args(format!(
            "Output must end in .{}",
            extensions.join(", .")
        )));
    }
    if !parent(path).is_dir() {
        return Err(io("Output parent directory does not exist"));
    }
    Ok(())
}

fn unit(text: Option<&str>) -> Result<DurationUnit> {
    match text {
        None => Ok(DurationUnit::Days),
        Some(t) => DurationUnit::parse(t).ok_or_else(|| args("unit must be days or hours")),
    }
}

/// A saved export template: a `.json` file, else a name saved in the
/// workspace database `input`.
fn load_template(input: &Path, spec: &str) -> Result<ExportTemplate> {
    let path = Path::new(spec);
    if spec.to_ascii_lowercase().ends_with(".json") || path.is_file() {
        let text = std::fs::read_to_string(path)
            .map_err(|e| args(format!("Cannot read the template file {spec:?}: {e}")))?;
        return export_template::parse_one(&text).map_err(invalid);
    }
    if !storage::is_database(input)? {
        return Err(args(
            "Template names are saved in a workspace; give a workspace input or a template .json file",
        ));
    }
    let c = storage::read_database(input)?;
    let text: Option<String> = c
        .query_row(
            "SELECT value FROM preferences WHERE key=?1",
            [export_template::PREFERENCE_KEY],
            |r| r.get(0),
        )
        .optional()
        .map_err(io)?;
    let (list, rejected) = export_template::parse_list(&text.unwrap_or_default());
    export_template::find(&list, spec).cloned().ok_or_else(|| {
        let mut message = format!("No saved export template is named {spec:?}");
        if list.is_empty() {
            message.push_str(" (the workspace has none)");
        } else {
            let names: Vec<&str> = list.iter().map(|t| t.name.as_str()).collect();
            message.push_str(&format!("; saved templates: {}", names.join(", ")));
        }
        for r in rejected {
            message.push_str(&format!(". {r}"));
        }
        AutomationError::new("template_not_found", message)
    })
}

/// `export-xlsx`: modern (default) or tables-layout spreadsheet of one project.
#[allow(clippy::too_many_arguments)]
pub(crate) fn export_xlsx(
    input: &Path,
    output: &Path,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    layout: Option<&str>,
    unit_text: Option<&str>,
    columns: Option<&str>,
    template: Option<&str>,
) -> Result<Value> {
    check_new_output(output, &["xlsx"])?;
    if template.is_some() && (columns.is_some() || unit_text.is_some()) {
        return Err(args(
            "A template sets the columns and the unit; leave out columns and unit",
        ));
    }
    if template.is_some() && layout.is_some_and(|l| l != "modern") {
        return Err(args("A template applies to the modern layout"));
    }
    let template = template
        .map(|spec| load_template(input, spec))
        .transpose()?;
    let loaded = storage::load(input, project_id, workspace_id)?;
    let p = &loaded.project;
    let unit = unit(unit_text)?;
    let lookup = Lookup::new(p);
    // "p6" is the old name of the tables layout, still accepted from old
    // scripts; the result always reports "tables".
    let layout = match layout.unwrap_or("modern") {
        "p6" => "tables",
        other => other,
    };
    let (mut workbook, summary) = match layout {
        "modern" if template.is_some() => {
            let t = template.as_ref().expect("checked");
            let options = t.options(p).map_err(invalid)?;
            modern_workbook(p, &options).map_err(invalid)?
        }
        "modern" => {
            let mut options = ExportOptions {
                unit,
                ..Default::default()
            };
            if let Some(list) = columns {
                options.columns = list
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|key| {
                        Column::from_key(key)
                            .or_else(|| openxer_interchange::columns::match_header(key, &lookup))
                            .ok_or_else(|| args(format!("Unknown column {key:?}")))
                    })
                    .collect::<Result<Vec<_>>>()?;
            }
            modern_workbook(p, &options).map_err(invalid)?
        }
        "tables" => tables_workbook(p, unit).map_err(invalid)?,
        _ => return Err(args("layout must be modern or tables")),
    };
    let temp = tempfile::Builder::new()
        .suffix(".xlsx")
        .tempfile_in(parent(output))
        .map_err(io)?;
    workbook.save(temp.path()).map_err(io)?;
    publish(temp, output)?;
    Ok(json!({"output":output,"layout":layout,"unit":unit.label(),
              "activities":summary.activities,"relationships":summary.relationships,
              "assignments":summary.assignments,"wbs_bands":summary.bands,
              "project":project_json(p)}))
}

/// `import-xlsx`: plan (and unless `dry_run`, apply) a spreadsheet to a project.
pub(crate) fn import_xlsx(
    input: &Path,
    spreadsheet: &Path,
    output: Option<&Path>,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    dry_run: bool,
) -> Result<Value> {
    match (output, dry_run) {
        (Some(_), true) => return Err(args("dry_run writes nothing; omit output")),
        (None, false) => return Err(args("output is required unless dry_run is true")),
        (Some(output), false) => storage::check_destination(output)?,
        (None, true) => {}
    }
    let mut loaded = storage::load(input, project_id, workspace_id)?;
    let (_, detected, _, plan) = xi::plan_file(spreadsheet, &loaded.project)
        .map_err(|e| AutomationError::new("invalid_spreadsheet", e))?;
    let counts = plan.counts();
    let rows: Vec<&xi::RowPlan> = plan
        .rows
        .iter()
        .filter(|r| !matches!(r.kind, xi::RowKind::Unchanged | xi::RowKind::Skipped))
        .collect();
    let links: Vec<&xi::LinkPlan> = plan
        .links
        .iter()
        .filter(|l| !matches!(l.kind, xi::RowKind::Unchanged | xi::RowKind::Skipped))
        .collect();
    let mut result = json!({"format":detected.format,"settings":detected.settings,"counts":counts,
        "rows":rows,"relationships":links,"notes":plan.notes,"dry_run":dry_run});
    if dry_run {
        return Ok(result);
    }
    let output = output.expect("checked above");
    let report = xi::apply(&plan, &mut loaded.project)
        .map_err(|e| AutomationError::new("import_rejected", e))?;
    storage::save(input, output, &loaded)?;
    result["output"] = json!(output);
    result["applied"] = json!(report);
    result["project"] = project_json(&loaded.project);
    Ok(result)
}

/// XER import into an existing workspace (copied to `output`): create new,
/// replace, or update a workspace project with per-type import actions.
#[allow(clippy::too_many_arguments)]
pub(crate) fn import_into_workspace(
    input: &Path,
    workspace: &Path,
    output: &Path,
    project_id: Option<i64>,
    action: &str,
    target: Option<i64>,
    config: Option<&str>,
    dictionary: Option<&str>,
) -> Result<Value> {
    storage::check_destination(output)?;
    // A workspace with a shared dictionary: what a calendar, resource or
    // code whose name the workspace has with other values does.
    let choices = dictionary
        .map(crate::workspace_cmd::parse_choices)
        .transpose()?
        .unwrap_or_default();
    if !storage::is_database_extension(output) {
        return Err(args(
            "import requires a database output (.db/.sqlite/.sqlite3)",
        ));
    }
    if !matches!(action, "new" | "update" | "replace" | "add") {
        return Err(args("action must be new, update, replace or add"));
    }
    if !storage::is_database(workspace)? {
        return Err(args("workspace must be an OpenXER workspace database"));
    }
    let config = match config {
        Some(text) => ImportConfig::parse(text).map_err(args)?,
        None => ImportConfig::default(),
    };
    let doc = openxer_interchange::project_xml::read_file(input).map_err(invalid)?;
    let selected: Vec<i64> = doc
        .projects()
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| project_id.is_none_or(|p| p == *id))
        .collect();
    if selected.is_empty() {
        return Err(AutomationError::new(
            "project_not_found",
            "No XER project matches the selector",
        ));
    }
    if action != "new" && selected.len() != 1 {
        return Err(args(
            "update, replace and add need exactly one XER project; pass project_id",
        ));
    }
    // The workspace keeps the file name, as a plain import does, not the
    // full local path.
    let source_name = input
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let temp = tempfile::NamedTempFile::new_in(parent(output)).map_err(io)?;
    let source = storage::read_database(workspace)?;
    let shared = storage::shared(&source)?;
    source.backup("main", temp.path(), None).map_err(io)?;
    let mut c = Connection::open(temp.path()).map_err(io)?;
    c.execute_batch("PRAGMA journal_mode=DELETE").map_err(io)?;
    let tx = c.transaction().map_err(io)?;
    storage::schema(&tx)?;
    let mut summaries = vec![];
    for pid in selected {
        match action {
            "new" => {
                let mut p = ProjectData::from_document(doc.clone(), pid, source_name.clone())
                    .map_err(invalid)?;
                p.model.validate().map_err(invalid)?;
                let mut source = p.source_name.clone();
                let mut n = 2;
                while tx
                    .query_row(
                        "SELECT 1 FROM projects WHERE source_name=?1 AND project_key=?2",
                        params![source, pid],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(io)?
                    .is_some()
                {
                    source = format!("{} ({n})", p.source_name);
                    n += 1;
                }
                p.source_name = source;
                let (id, events) = storage::store_with(&tx, &p, None, None, &choices)?;
                summaries.push(json!({"action":"new","workspace_id":id,"project":project_json(&p),"warnings":p.warnings,
                    "dictionary":crate::workspace_db::import_report(&events)}));
            }
            "update" | "replace" | "add" => {
                let rows = storage::rows(&tx)?;
                let id = match target {
                    Some(id) => id,
                    None => {
                        let matching: Vec<i64> = rows
                            .iter()
                            .filter(|r| r["project_id"] == pid)
                            .filter_map(|r| r["workspace_id"].as_i64())
                            .collect();
                        if matching.len() != 1 {
                            return Err(args("pass target_workspace_id; the project id does not identify one workspace project"));
                        }
                        matching[0]
                    }
                };
                let (source, key, text, legacy, level): (String, i64, String, bool, bool) = tx
                    .query_row(
                        "SELECT source_name,project_key,xer_content,source_windows_encoding,level_resources FROM projects WHERE id=?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                    )
                    .optional()
                    .map_err(io)?
                    .ok_or_else(|| AutomationError::new("project_not_found", format!("Workspace row {id} does not exist")))?;
                let mut target_doc = Document::parse(&text).map_err(invalid)?;
                target_doc.windows_encoding = legacy;
                // The target's shared rows come back from the dictionary
                // before the file is merged into it.
                let mark = if shared {
                    Some(
                        crate::workspace_db::hydrate(&tx, &mut target_doc, Some(id))
                            .map_err(invalid)?,
                    )
                } else {
                    None
                };
                let (mut p, report) = if action == "update" || action == "add" {
                    let merged = if action == "add" {
                        openxer_interchange::xer_merge::add_into_project(
                            &target_doc,
                            key,
                            &doc,
                            pid,
                            &config,
                        )
                    } else {
                        merge_project(&target_doc, key, &doc, pid, &config)
                    };
                    let (merged, report) =
                        merged.map_err(|e| AutomationError::new("import_rejected", e))?;
                    let mut p =
                        ProjectData::from_document(merged, key, source.clone()).map_err(invalid)?;
                    p.document.windows_encoding = legacy;
                    (p, json!(report))
                } else {
                    let p = ProjectData::from_document(doc.clone(), pid, source.clone())
                        .map_err(invalid)?;
                    (p, Value::Null)
                };
                p.model.options.do_level = level;
                p.model.validate().map_err(invalid)?;
                // Replace reads the project whole from the file: none of its
                // dictionary rows are the ones the target read.
                let mark = mark.filter(|_| action != "replace");
                tx.execute(
                    "UPDATE projects SET project_key=?1 WHERE id=?2",
                    params![p.model.project.proj_id, id],
                )
                .map_err(io)?;
                let (_, events) = storage::store_with(&tx, &p, Some(id), mark.as_ref(), &choices)?;
                summaries.push(json!({"action":action,"workspace_id":id,"project":project_json(&p),"report":report,"warnings":p.warnings,
                    "dictionary":crate::workspace_db::import_report(&events)}));
            }
            _ => return Err(args("action must be new, update, replace or add")),
        }
    }
    tx.commit().map_err(io)?;
    c.close().map_err(|(_, e)| io(e))?;
    publish(temp, output)?;
    Ok(json!({"output":output,"workspace":workspace,"projects":summaries}))
}

/// Whether an export writes project XML: `format` when given (`xer` or
/// `xml`), else an output that ends in `.xml`.
pub(crate) fn writes_xml(format: Option<&str>, output: &Path) -> Result<bool> {
    match format.map(|f| f.trim().to_ascii_lowercase()).as_deref() {
        Some("xml") => Ok(true),
        Some("xer") => Ok(false),
        Some(other) => Err(args(format!("format must be xer or xml, not {other:?}"))),
        None => Ok(output
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xml"))),
    }
}

/// With `format`, refuse an input file that is not in that format (the
/// import tells XER from XML by the file's contents).
pub(crate) fn check_input_format(input: &Path, format: Option<&str>) -> Result<()> {
    let Some(format) = format else {
        return Ok(());
    };
    let want_xml = match format.trim().to_ascii_lowercase().as_str() {
        "xml" => true,
        "xer" => false,
        other => return Err(args(format!("format must be xer or xml, not {other:?}"))),
    };
    let mut head = [0u8; 64];
    let n = std::fs::File::open(input)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut head))
        .map_err(io)?;
    if openxer_interchange::project_xml::is_xml(&head[..n]) != want_xml {
        return Err(AutomationError::new(
            "invalid_input",
            format!(
                "The input is not a {} file",
                format.trim().to_ascii_uppercase()
            ),
        ));
    }
    Ok(())
}

/// Write an export document as XER or as project XML.
fn write_export(doc: &Document, xml: bool, path: &Path) -> Result<()> {
    if xml {
        openxer_interchange::project_xml::write_file(doc, path)
            .map_err(|e| AutomationError::new("export_rejected", e))
    } else {
        doc.write(path).map_err(io)
    }
}

/// The plain export (the loaded project's document, other projects and
/// unknown tables kept) with the header version label `version` and today's
/// export date. `xml` writes it as project XML instead of XER.
pub(crate) fn export_with_version(
    input: &Path,
    output: &Path,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    version: &str,
    xml: bool,
) -> Result<Value> {
    if storage::is_database_extension(output) {
        return Err(args("export requires an .xer or .xml output"));
    }
    storage::check_destination_as(output, xml)?;
    let loaded = storage::load(input, project_id, workspace_id)?;
    // XML carries no unknown tables, so from a shared workspace dictionary
    // it takes only the rows the project uses, as the plain export does.
    let mut doc = if xml {
        storage::export_document(&loaded)
    } else {
        loaded.project.updated_document()
    };
    let today = openxer_xer::edit::project_records::today();
    doc.header = openxer_interchange::xer_export::with_version(&doc.header, version, &today[..10]);
    let temp = tempfile::NamedTempFile::new_in(parent(output)).map_err(io)?;
    write_export(&doc, xml, temp.path())?;
    publish(temp, output)?;
    Ok(
        json!({"output":output,"format":if xml { "xml" } else { "xer" },
              "version":openxer_interchange::xer_export::header_version(&doc.header),
              "project":project_json(&loaded.project),"warnings":loaded.project.warnings}),
    )
}

/// XER (or, with `xml`, project XML) export of one or several projects
/// with an explicit version label.
pub(crate) fn export_projects(
    input: &Path,
    output: &Path,
    projects: &str,
    version: &str,
    xml: bool,
) -> Result<Value> {
    if storage::is_database_extension(output) {
        return Err(args("export requires an .xer or .xml output"));
    }
    storage::check_destination_as(output, xml)?;
    let mut loaded = vec![];
    for part in projects.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let id: i64 = part
            .parse()
            .map_err(|_| args(format!("project {part:?} is not an integer")))?;
        // A workspace id first (as documented); a project id only when no
        // workspace row has that id. Any other failure is reported.
        let p = if storage::is_database(input)? {
            match storage::load(input, None, Some(id)) {
                Err(e) if e.code == "project_not_found" => storage::load(input, Some(id), None)
                    .map_err(|e| {
                        if e.code == "project_not_found" {
                            AutomationError::new(
                                "project_not_found",
                                format!("No workspace row or project has the id {id}"),
                            )
                        } else {
                            e
                        }
                    })?,
                other => other?,
            }
        } else {
            storage::load(input, Some(id), None)?
        };
        loaded.push(p);
    }
    if loaded.is_empty() {
        return Err(args("projects must list at least one id"));
    }
    let refs: Vec<&ProjectData> = loaded.iter().map(|l| &l.project).collect();
    let mut doc =
        export_document(&refs, version).map_err(|e| AutomationError::new("export_rejected", e))?;
    // From a shared workspace dictionary: only the rows these projects use.
    if loaded.iter().any(|l| l.mark.is_some()) {
        let pids: Vec<i64> = refs.iter().map(|p| p.model.project.proj_id).collect();
        doc = openxer_xer::workspace_scope::used_subset_of(&doc, &pids);
    }
    let temp = tempfile::NamedTempFile::new_in(parent(output)).map_err(io)?;
    write_export(&doc, xml, temp.path())?;
    publish(temp, output)?;
    Ok(
        json!({"output":output,"format":if xml { "xml" } else { "xer" },
              "version":openxer_interchange::xer_export::header_version(&doc.header),
              "projects":refs.iter().map(|p| project_json(p)).collect::<Vec<_>>()}),
    )
}
