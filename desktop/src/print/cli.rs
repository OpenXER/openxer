//! `OpenXER export-pdf`: prints a layout to a PDF without opening a window,
//! with the same renderer as Print Preview. The workspace is copied to a
//! temporary file first, so opening a named layout never changes it.
use super::setup::{Orientation, PageSetup};
use crate::*;
use serde_json::{json, Value};

pub const USAGE: &str = "Usage: OpenXER export-pdf --output FILE.pdf
         (--database WORKSPACE.db [--workspace-id ID] | --input FILE.xer [--project ID])
         [--layout NAME] [--view gantt|network] [--paper NAME]
         [--orientation portrait|landscape|automatic]
The output must not exist. Without --layout the workspace's current layout prints.
A saved Same as screen setup has no screen to copy here and prints its
Timescale Start and Finish; pages run down only unless the setup allows
pages side by side.";

fn flags(args: &[String]) -> Result<BTreeMap<String, String>, String> {
    const ALLOWED: [&str; 9] = [
        "output",
        "database",
        "workspace-id",
        "input",
        "project",
        "layout",
        "view",
        "paper",
        "orientation",
    ];
    let mut out = BTreeMap::new();
    let mut k = 0;
    while k < args.len() {
        let name = args[k]
            .strip_prefix("--")
            .ok_or_else(|| format!("Unexpected argument {:?}\n{USAGE}", args[k]))?;
        if !ALLOWED.contains(&name) {
            return Err(format!("Unknown flag --{name}\n{USAGE}"));
        }
        let value = args
            .get(k + 1)
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| format!("--{name} requires a value"))?;
        if out.insert(name.to_string(), value.clone()).is_some() {
            return Err(format!("Duplicate flag --{name}"));
        }
        k += 2;
    }
    Ok(out)
}

fn temp_dir() -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir =
        std::env::temp_dir().join(format!("openxer-export-pdf-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Runs the command; returns the JSON summary.
pub fn run(args: &[String]) -> Result<Value, String> {
    let f = flags(args)?;
    let output = PathBuf::from(
        f.get("output")
            .ok_or_else(|| format!("--output is required\n{USAGE}"))?,
    );
    if output
        .extension()
        .and_then(|e| e.to_str())
        .is_none_or(|e| !e.eq_ignore_ascii_case("pdf"))
    {
        return Err("--output must end in .pdf".into());
    }
    if output.exists() {
        return Err(format!("Destination already exists: {}", output.display()));
    }
    if f.contains_key("database") == f.contains_key("input") {
        return Err(format!(
            "Give exactly one of --database or --input\n{USAGE}"
        ));
    }
    let dir = temp_dir()?;
    let result = export(&f, &output, &dir.join("workspace.db"));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn export(f: &BTreeMap<String, String>, output: &Path, copy: &Path) -> Result<Value, String> {
    let number = |key: &str| -> Result<Option<i64>, String> {
        f.get(key)
            .map(|v| {
                v.parse::<i64>()
                    .map_err(|_| format!("--{key} must be a number"))
            })
            .transpose()
    };
    let project = if let Some(db) = f.get("database") {
        let source = rusqlite::Connection::open_with_flags(
            db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|e| format!("Cannot open {db}: {e}"))?;
        source
            .execute("VACUUM INTO ?1", [copy.display().to_string()])
            .map_err(|e| format!("Cannot copy {db}: {e}"))?;
        drop(source);
        let store = Store::open(copy.to_path_buf())?;
        let p = match number("workspace-id")? {
            Some(id) => store.load(id)?,
            None => store.last()?.ok_or("The workspace holds no project")?,
        };
        (store, p)
    } else {
        let input = &f["input"];
        let doc = Document::read(Path::new(input))?;
        let projects = openxer_interchange::baseline::schedule_projects(&doc);
        let id = match number("project")? {
            Some(id) => id,
            None if projects.len() == 1 => projects[0].0,
            None => return Err("The XER holds several projects; choose one with --project".into()),
        };
        let mut p = ProjectData::from_document(doc, id, input.clone())?;
        if p.model.activities.iter().all(|a| a.early_start.is_none()) {
            schedule_model(&mut p.model, &p.resources)?;
        }
        let mut store = Store::open(copy.to_path_buf())?;
        store.save(&p)?;
        (store, p)
    };
    let (store, project) = project;
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    app.adopt_file_baselines();
    if let Some(name) = f.get("layout") {
        app.open_layout(name, true)?;
    }
    match f.get("view").map(String::as_str) {
        None => {}
        Some("gantt") => app.set_top(panes::TopView::Gantt),
        Some("network") => app.set_top(panes::TopView::Network),
        Some(other) => return Err(format!("--view must be gantt or network, not {other:?}")),
    }
    let mut setup: PageSetup = app.page_setup();
    if let Some(paper) = f.get("paper") {
        let p = super::setup::PAPERS
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(paper))
            .ok_or_else(|| format!("Unknown paper {paper:?}"))?;
        setup.paper = p.name.into();
    }
    match f.get("orientation").map(|s| s.to_ascii_lowercase()) {
        None => {}
        Some(o) if o == "portrait" => setup.orientation = Orientation::Portrait,
        Some(o) if o == "landscape" => setup.orientation = Orientation::Landscape,
        Some(o) if o == "automatic" => setup.orientation = Orientation::Automatic,
        Some(o) => {
            return Err(format!(
                "--orientation must be portrait, landscape or automatic, not {o:?}"
            ))
        }
    }
    let printed = app.print_with(&setup)?;
    let title = format!(
        "{} - {}",
        app.project.model.project.name, app.grid.layout.name
    );
    // Created exclusively: an existing file or link is never overwritten.
    // The name is taken first; the pages then go in whole through a
    // temporary file, and a failed write leaves no part-written PDF.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!("Destination already exists: {}", output.display())
            } else {
                format!("Cannot create {}: {e}", output.display())
            }
        })?;
    drop(file);
    if let Err(e) =
        openxer_xer::write_file_atomic(output, &super::pdf::write(&printed.pages, &title))
    {
        let _ = std::fs::remove_file(output);
        return Err(format!("Cannot write {}: {e}", output.display()));
    }
    let mut shown = setup.clone();
    shown.orientation = printed.orientation;
    let (w, h) = shown.page_inches();
    Ok(json!({
        "output": output.display().to_string(),
        "pages": printed.pages.len(),
        "pages_across": printed.across,
        "layout": app.grid.layout.name,
        "view": if app.panes.top == panes::TopView::Network { "network" } else { "gantt" },
        "paper": setup.paper,
        "orientation": if printed.orientation == Orientation::Portrait { "portrait" } else { "landscape" },
        "page_inches": [w, h],
        "scale": printed.scale,
        "pixels_per_day": printed.pixels_per_day,
        "timescale": printed.timescale.map(|(a, b)| [
            format::date(app.project.model.origin_serial, Some(a)),
            format::date(app.project.model.origin_serial, Some(b)),
        ]),
        "activities": app.project.model.activities.len(),
    }))
}
