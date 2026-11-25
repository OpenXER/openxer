//! Interchange wizard flows driven through the production App, plus software
//! renders of each wizard page (PNG files in the gitignored artifacts folder).
use super::*;
use crate::headless_render::SoftwarePainter;
use openxer_interchange::xlsx_import::Cell;

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/interchange");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-ix-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn app(dir: &Path) -> (App, egui::Context) {
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, scheduled_starter());
    app.save();
    (app, ctx)
}

#[test]
fn spreadsheet_import_applies_as_one_undoable_step() {
    let dir = temp("sheet");
    let (mut app, ctx) = app(&dir);
    let file = dir.join("layout.xlsx");
    app.export_excel_to(&file);
    assert!(file.exists(), "{}", app.status);
    // Edit the exported file: rename one activity and change a remaining duration.
    let mut wb = xi::read_workbook(&file).unwrap();
    let s = wb.sheet("Activities").unwrap();
    let headers: Vec<String> = wb.sheets[s].rows[0].iter().map(Cell::text).collect();
    let name_col = headers.iter().position(|h| h == "Activity Name").unwrap();
    let rd_col = headers
        .iter()
        .position(|h| h == "Remaining Duration (d)")
        .unwrap();
    let row = wb.sheets[s]
        .rows
        .iter()
        .position(|r| r[0].text() == "RB-1010")
        .unwrap();
    wb.sheets[s].rows[row][name_col] = Cell::Text("Survey (edited in Excel)".into());
    wb.sheets[s].rows[row][rd_col] = Cell::Number(5.0);
    let before_model = app.project.model.clone();
    app.open_sheet_import(file.display().to_string());
    let lookup = Lookup::new(&app.project);
    let detected = xi::detect(&wb, &lookup).unwrap();
    let mapping = xi::auto_map(&detected, &lookup);
    {
        let w = app.interchange.sheet.as_mut().unwrap();
        w.wb = Some(wb);
        w.detected = Some(detected);
        w.mapping = Some(mapping);
        w.step = 1;
    }
    app.sheet_plan();
    let counts = app
        .interchange
        .sheet
        .as_ref()
        .unwrap()
        .plan
        .as_ref()
        .unwrap()
        .counts();
    assert_eq!(counts.changed, 1);
    app.sheet_apply(1.0);
    assert!(app.interchange.sheet.is_none());
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_code == "RB-1010")
        .unwrap();
    assert_eq!(app.project.name(i), "Survey (edited in Excel)");
    assert_eq!(app.project.model.activities[i].rem, 5 * 8 * 60);
    assert_eq!(
        app.history.counts().0,
        1,
        "one undo step for the whole import"
    );
    app.history_move(&ctx, false);
    assert_eq!(app.project.model, before_model);
    assert_ne!(app.project.name(i), "Survey (edited in Excel)");
    app.history_move(&ctx, true);
    assert_eq!(app.project.name(i), "Survey (edited in Excel)");
}

#[test]
fn xer_import_create_update_replace_and_undo_through_the_wizard() {
    let dir = temp("xer");
    let (mut app, _ctx) = app(&dir);
    let file = dir.join("update.xer");
    let mut edited = app.project.clone();
    edited.details.get_mut(&1020).unwrap().name = "Design (contractor update)".into();
    edited.updated_document().write(&file).unwrap();
    let rows_before = workspace::rows(&app.store).unwrap().len();

    // Create New.
    app.open_import_wizard();
    app.interchange.xer.as_mut().unwrap().path_text = file.display().to_string();
    app.xer_load_files();
    assert_eq!(app.interchange.xer.as_ref().unwrap().rows.len(), 1);
    app.xer_prepare();
    app.xer_finish();
    assert!(app.interchange.message.is_some());
    assert_eq!(workspace::rows(&app.store).unwrap().len(), rows_before + 1);
    assert_eq!(
        app.project.details[&1020].name,
        "Design (contractor update)"
    );
    app.undo_last_import();
    assert_eq!(workspace::rows(&app.store).unwrap().len(), rows_before);

    // Update Existing into the original workspace project.
    let original = workspace::rows(&app.store).unwrap()[0].id;
    let p = app.store.load(original).unwrap();
    app.dirty = false;
    app.load(p);
    app.open_import_wizard();
    {
        let w = app.interchange.xer.as_mut().unwrap();
        w.path_text = file.display().to_string();
    }
    app.xer_load_files();
    {
        let w = app.interchange.xer.as_mut().unwrap();
        w.rows[0].action = ImportAction::UpdateExisting;
        w.rows[0].target = Some(original);
        assert!(App::xer_validate_rows(w).is_none());
    }
    app.xer_prepare();
    let summary = app.interchange.xer.as_ref().unwrap().prepared[0]
        .summary
        .join("\n");
    assert!(
        summary.contains("Activities: 0 added, 1 updated"),
        "{summary}"
    );
    app.xer_finish();
    assert_eq!(
        app.store.load(original).unwrap().details[&1020].name,
        "Design (contractor update)"
    );
    assert_eq!(
        app.project.details[&1020].name,
        "Design (contractor update)"
    );
    app.undo_last_import();
    assert_ne!(
        app.store.load(original).unwrap().details[&1020].name,
        "Design (contractor update)"
    );
    assert_ne!(
        app.project.details[&1020].name,
        "Design (contractor update)"
    );

    // Replace Existing, and a conflicting double target is refused.
    app.open_import_wizard();
    app.interchange.xer.as_mut().unwrap().path_text =
        format!("{}; {}", file.display(), file.display());
    app.xer_load_files();
    {
        let w = app.interchange.xer.as_mut().unwrap();
        for r in &mut w.rows {
            r.action = ImportAction::ReplaceExisting;
            r.target = Some(original);
        }
        assert!(App::xer_validate_rows(w).unwrap().contains("Conflicting"));
        w.rows[1].action = ImportAction::Ignore;
        assert!(App::xer_validate_rows(w).is_none());
    }
    app.xer_prepare();
    app.xer_finish();
    assert_eq!(
        app.store.load(original).unwrap().details[&1020].name,
        "Design (contractor update)"
    );
    let backups = std::fs::read_dir(dir.join("backups"))
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(workspace::IMPORT_BACKUP)
        })
        .count();
    assert_eq!(backups, 3, "each import backs up the workspace first");
}

/// Add Into Existing Project: the file's activities land in the chosen
/// project beside its own, colliding IDs renumbered; Undo Last Import takes
/// them out again.
#[test]
fn xer_import_add_into_existing_project_and_undo() {
    let dir = temp("addinto");
    let (mut app, _ctx) = app(&dir);
    let file = dir.join("more.xer");
    app.project.updated_document().write(&file).unwrap();
    let original = workspace::rows(&app.store).unwrap()[0].id;
    let before = app.project.model.activities.len();
    let codes_before: BTreeSet<String> = app
        .project
        .model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .collect();
    app.open_import_wizard();
    app.interchange.xer.as_mut().unwrap().path_text = file.display().to_string();
    app.xer_load_files();
    {
        let w = app.interchange.xer.as_mut().unwrap();
        w.rows[0].action = ImportAction::AddInto;
        w.rows[0].target = None;
        assert!(App::xer_validate_rows(w).unwrap().contains("Import To"));
        w.rows[0].target = Some(original);
        assert!(App::xer_validate_rows(w).is_none());
    }
    app.xer_prepare();
    let prepared = &app.interchange.xer.as_ref().unwrap().prepared[0];
    assert!(prepared.error.is_none(), "{:?}", prepared.error);
    assert!(
        prepared.summary[0].contains(&format!("Adds {before} activities")),
        "{:?}",
        prepared.summary
    );
    app.xer_finish();
    let stored = app.store.load(original).unwrap();
    assert_eq!(stored.model.activities.len(), 2 * before);
    assert_eq!(app.project.model.activities.len(), 2 * before, "reopened");
    let codes: BTreeSet<String> = stored
        .model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .collect();
    assert_eq!(codes.len(), 2 * before, "fresh IDs where they collided");
    assert!(codes.is_superset(&codes_before));
    app.undo_last_import();
    assert_eq!(
        app.store.load(original).unwrap().model.activities.len(),
        before
    );
    assert_eq!(app.project.model.activities.len(), before);
}

#[test]
fn export_wizard_writes_xer_and_both_spreadsheets() {
    let dir = temp("export");
    let (mut app, _ctx) = app(&dir);
    for (format, name) in [
        (ExportFormat::Xer, "out.xer"),
        (ExportFormat::Modern, "out-modern.xlsx"),
        (ExportFormat::Tables, "out-tables.xlsx"),
    ] {
        app.open_export_wizard();
        {
            let w = app.interchange.export.as_mut().unwrap();
            assert!(
                w.chosen.iter().any(|c| *c),
                "the open project is preselected"
            );
            w.format = format;
            w.version = "19.12".into();
            w.path = dir.join(name).display().to_string();
            w.step = 3;
        }
        app.export_finish();
        assert!(
            app.interchange.export.is_none(),
            "{:?}",
            app.interchange
                .export
                .as_ref()
                .and_then(|w| w.error.clone())
        );
        app.interchange.message = None;
    }
    let doc = Document::read(&dir.join("out.xer")).unwrap();
    assert!(doc.header.starts_with("ERMHDR\t19.12\t"));
    let back = ProjectData::from_document(doc, 1, "x".into()).unwrap();
    assert_eq!(
        back.model.activities.len(),
        app.project.model.activities.len()
    );
    let (_, d, _, plan) = xi::plan_file(&dir.join("out-modern.xlsx"), &app.project).unwrap();
    assert_eq!(d.format, xi::Format::OpenXer);
    assert!(!plan.has_changes());
    let (_, d, _, plan) = xi::plan_file(&dir.join("out-tables.xlsx"), &app.project).unwrap();
    assert_eq!(d.format, xi::Format::Tables);
    assert!(!plan.has_changes());
}

type Setup = Box<dyn Fn(&mut App)>;

/// Software renders of every wizard page into artifacts/interchange.
#[test]
fn render_interchange_wizards() {
    let dir = temp("render");
    let (mut app, ctx) = app(&dir);
    let out = out_dir();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    let xer = dir.join("render.xer");
    app.project.updated_document().write(&xer).unwrap();
    let sheet = dir.join("render.xlsx");
    app.export_excel_to(&sheet);
    let mut wb = xi::read_workbook(&sheet).unwrap();
    let s = wb.sheet("Activities").unwrap();
    wb.sheets[s].rows[2][1] = Cell::Text("Renamed in the spreadsheet".into());
    let rd = wb.sheets[s].rows[0]
        .iter()
        .position(|c| c.text() == "Remaining Duration (d)")
        .unwrap();
    wb.sheets[s].rows[3][rd] = Cell::Text("lots".into());
    let mut shots: Vec<(String, Setup)> = vec![];
    for step in 0..5 {
        let xer = xer.clone();
        shots.push((
            format!("xer-import-{step}.png"),
            Box::new(move |app: &mut App| {
                app.open_import_wizard();
                app.interchange.xer.as_mut().unwrap().path_text = xer.display().to_string();
                if step >= 2 {
                    app.xer_load_files();
                    let w = app.interchange.xer.as_mut().unwrap();
                    w.rows[0].action = ImportAction::UpdateExisting;
                    w.rows[0].target = w.workspace.first().map(|r| r.id);
                }
                if step == 4 {
                    app.xer_prepare();
                }
                app.interchange.xer.as_mut().unwrap().step = step;
            }),
        ));
    }
    for step in 0..4 {
        shots.push((
            format!("export-{step}.png"),
            Box::new(move |app: &mut App| {
                app.open_export_wizard();
                let w = app.interchange.export.as_mut().unwrap();
                w.step = step;
                if step == 3 {
                    w.path = "C:\\Schedules\\RIVERBEND.xer".into();
                }
            }),
        ));
    }
    for step in 0..3 {
        let sheet = sheet.clone();
        let wb = wb.clone();
        shots.push((
            format!("sheet-import-{step}.png"),
            Box::new(move |app: &mut App| {
                app.open_sheet_import(sheet.display().to_string());
                if step >= 1 {
                    let lookup = Lookup::new(&app.project);
                    let d = xi::detect(&wb, &lookup).unwrap();
                    let m = xi::auto_map(&d, &lookup);
                    let w = app.interchange.sheet.as_mut().unwrap();
                    w.wb = Some(wb.clone());
                    w.detected = Some(d);
                    w.mapping = Some(m);
                    w.step = 1;
                }
                if step == 2 {
                    app.sheet_plan();
                }
            }),
        ));
    }
    shots.push((
        "import-message.png".into(),
        Box::new(|app: &mut App| {
            app.interchange.message = Some(Message {
                title: "Import".into(),
                text: "The import is complete. 1 project imported. Warnings were recorded:".into(),
                lines: vec![
                    "Update Existing Project: RIVERBEND: 2 cross-project relationships use stored external date limits.".into(),
                    "Backup of the workspace before the import: C:\\OpenXER\\backups\\openxer-before-import.db".into(),
                ],
            });
        }),
    ));
    for (name, setup) in shots {
        app.interchange = Interchange::default();
        setup(&mut app);
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, triangles) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        assert!(triangles > 100, "{name} painted almost nothing");
        image.save(out.join(&name)).unwrap();
    }
    // Clicking Cancel closes the wizard.
    app.interchange = Interchange::default();
    app.open_export_wizard();
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    painter.click_action(&mut app, &ctx, &mut time, "export_cancel");
    assert!(app.interchange.export.is_none());
}

#[test]
fn xer_wizard_imports_and_exports_baselines_with_their_project() {
    use openxer_interchange::baseline as xb;
    let dir = temp("baselines");
    let (mut app, _ctx) = app(&dir);
    let pid = app.project.model.project.proj_id;
    let mut doc = app.project.updated_document();
    let copy = xb::copy_as_baseline(&doc, pid, 900, "RB - BL", "Baseline", "2026-10-01 08:00");
    xb::append_project(&mut doc, &copy, 900);
    let file = dir.join("with-baseline.xer");
    doc.write(&file).unwrap();
    let count = |app: &App| -> i64 {
        app.store
            .connection()
            .query_row("SELECT COUNT(*) FROM baselines", [], |r| r.get(0))
            .unwrap()
    };
    let import = |app: &mut App| {
        app.open_import_wizard();
        app.interchange.xer.as_mut().unwrap().path_text = file.display().to_string();
        app.xer_load_files();
        let w = app.interchange.xer.as_ref().unwrap();
        assert_eq!(w.rows.len(), 1, "the baseline travels with its project");
        app.xer_prepare();
        app.xer_finish();
    };
    import(&mut app);
    let message = app.interchange.message.take().unwrap();
    assert!(
        message
            .lines
            .iter()
            .any(|l| l.contains("1 baseline stored")),
        "{:?}",
        message.lines
    );
    assert_eq!(count(&app), 1);
    app.undo_last_import();
    assert_eq!(count(&app), 0, "undo removes the imported baseline");

    import(&mut app);
    let row = workspace::rows(&app.store)
        .unwrap()
        .into_iter()
        .find(|r| r.source == file.display().to_string())
        .unwrap();
    let out = dir.join("export.xer");
    app.open_export_wizard();
    {
        let w = app.interchange.export.as_mut().unwrap();
        let k = w.workspace.iter().position(|r| r.id == row.id).unwrap();
        assert_eq!(w.baseline_counts[k], 1);
        for (n, c) in w.chosen.iter_mut().enumerate() {
            *c = n == k;
        }
        w.baselines[k] = true;
        w.path = out.display().to_string();
    }
    app.export_finish();
    assert!(app.interchange.export.is_none(), "{}", app.status);
    assert!(app.status.contains("with 1 baseline "), "{}", app.status);
    let back = Document::read(&out).unwrap();
    assert_eq!(xb::baselines_in(&back), vec![(900, pid)]);
    assert_eq!(xb::schedule_projects(&back).len(), 1);
}

/// Everything a layout stores about how the Activities view looks: the
/// layout (columns, filters, grouping, bands, table font and row), the Gantt
/// settings (bars, chart font and colors, timescale) and the timescale units.
fn appearance(app: &App) -> (serde_json::Value, String, String) {
    let mut layout = app.grid.layout.to_json();
    layout["name"] = serde_json::Value::Null;
    (
        layout,
        app.gantt.to_json(),
        format!(
            "{:?} {:?} {} {}",
            app.view.upper,
            app.view.lower,
            app.view.automatic_units,
            app.panes.network.options.json()
        ),
    )
}

/// Compare part by part, naming the part that changed.
fn assert_same_appearance(
    now: &(serde_json::Value, String, String),
    custom: &(serde_json::Value, String, String),
    step: &str,
) {
    for key in ["columns", "filter", "group_sort", "table"] {
        assert_eq!(now.0[key], custom.0[key], "{step}: layout {key}");
    }
    let a: serde_json::Value = serde_json::from_str(&now.1).unwrap();
    let b: serde_json::Value = serde_json::from_str(&custom.1).unwrap();
    for key in ["bars", "timescale", "options", "curtains", "texts"] {
        assert_eq!(a[key], b[key], "{step}: gantt {key}");
    }
    assert_eq!(
        now.2, custom.2,
        "{step}: timescale units and network options"
    );
}

fn run_frames(app: &mut App, ctx: &egui::Context, time: &mut f64, n: usize) {
    for _ in 0..n {
        *time += 0.5;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
                time: Some(*time),
                focused: true,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
    }
}

fn customize_appearance(app: &mut App) {
    use crate::gantt::settings::{DateInterval, Rgb};
    let g = &mut app.grid.layout;
    g.group_sort.bands[0].background = [0x10, 0x20, 0x30];
    g.group_sort.bands[0].text = [0xF0, 0xE0, 0xD0];
    g.group_sort.bands[0].font_size = 11.;
    g.group_sort.bands[0].row_height = 22.;
    g.group_sort.bands[1].background = [0x90, 0x80, 0x70];
    g.table.font.points = 10.;
    g.table.font.bold = true;
    g.columns.truncate(5);
    g.group_sort.show_grand_totals = true;
    let mut settings = app.gantt.clone();
    for bar in &mut settings.bars {
        if bar.name == "Remaining Work" {
            bar.middle.color = Rgb(0x12, 0x34, 0x56);
        }
        if bar.name == "Milestone" {
            bar.start.color = Rgb(0x65, 0x43, 0x21);
        }
    }
    settings.options.font.points = 10.;
    settings.options.font.bold = true;
    settings.options.background = Rgb(250, 250, 240);
    settings.options.data_date.color = Rgb(0x22, 0x44, 0x66);
    settings.timescale.interval = DateInterval::QuarterMonth;
    app.apply_gantt(settings).unwrap();
    app.panes.network.options.normal = egui::Color32::from_rgb(0x33, 0x99, 0x66);
    app.panes.network.options.font_size = 11.;
}

/// Customized colors, fonts, bars, columns, grouping and timescale survive
/// every way of bringing in or opening a project, and a reopened workspace.
#[test]
fn customized_appearance_survives_imports_opens_and_reopen() {
    let dir = temp("appearance");
    let (mut app, ctx) = app(&dir);
    let mut time = 0.;
    run_frames(&mut app, &ctx, &mut time, 3);
    customize_appearance(&mut app);
    run_frames(&mut app, &ctx, &mut time, 3);
    let custom = appearance(&app);
    assert_ne!(custom.0, crate::grid::layout::Layout::default().to_json());

    // Import XER, Create New (opens the new project).
    let file = dir.join("new.xer");
    let mut other = app.project.clone();
    other.details.get_mut(&1020).unwrap().name = "Design (new project)".into();
    other.updated_document().write(&file).unwrap();
    app.open_import_wizard();
    app.interchange.xer.as_mut().unwrap().path_text = file.display().to_string();
    app.xer_load_files();
    app.interchange.xer.as_mut().unwrap().open_after = true;
    app.xer_prepare();
    app.xer_finish();
    run_frames(&mut app, &ctx, &mut time, 3);
    assert_same_appearance(&appearance(&app), &custom, "after Import XER");

    // Open another project from the workspace.
    let ids: Vec<i64> = workspace::rows(&app.store)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    let first = ids[0];
    let p = app.store.load(first).unwrap();
    app.dirty = false;
    app.load(p);
    run_frames(&mut app, &ctx, &mut time, 3);
    assert_same_appearance(&appearance(&app), &custom, "after opening a project");
    app.open_project_set(&ids);
    run_frames(&mut app, &ctx, &mut time, 3);
    assert_same_appearance(&appearance(&app), &custom, "after opening several projects");

    // Add Into Existing Project.
    app.open_import_wizard();
    app.interchange.xer.as_mut().unwrap().path_text = file.display().to_string();
    app.xer_load_files();
    {
        let w = app.interchange.xer.as_mut().unwrap();
        w.rows[0].action = ImportAction::AddInto;
        w.rows[0].target = Some(first);
    }
    app.xer_prepare();
    app.xer_finish();
    run_frames(&mut app, &ctx, &mut time, 3);
    assert_same_appearance(
        &appearance(&app),
        &custom,
        "after Add Into Existing Project",
    );

    // Spreadsheet import.
    let sheet = dir.join("sheet.xlsx");
    app.export_excel_to(&sheet);
    app.open_sheet_import(sheet.display().to_string());
    run_frames(&mut app, &ctx, &mut time, 3);
    app.interchange.sheet = None;
    assert_same_appearance(&appearance(&app), &custom, "after the spreadsheet import");

    // Reopen the workspace.
    app.save();
    let project = app.project.clone();
    drop(app);
    let store = Store::open(dir.join("workspace.db")).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    run_frames(&mut app, &ctx, &mut time, 3);
    assert_same_appearance(&appearance(&app), &custom, "after reopening the workspace");
}

#[test]
fn export_wizard_saves_and_reruns_spreadsheet_templates() {
    use openxer_interchange::export_template::{self as et, Condition, Op, SortKey};
    let dir = temp("templates");
    let (mut app, ctx) = app(&dir);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    app.open_export_wizard();
    {
        let w = app.interchange.export.as_mut().unwrap();
        w.format = ExportFormat::Modern;
        w.step = 2;
    }
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    // New... opens the editor; Cancel leaves nothing saved.
    painter.click_action(&mut app, &ctx, &mut time, "export_template_new");
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    assert!(app.interchange.export.as_ref().unwrap().editor.is_some());
    painter.click_action(&mut app, &ctx, &mut time, "template_cancel");
    assert!(app.interchange.export.as_ref().unwrap().editor.is_none());
    assert!(app.store.preference(et::PREFERENCE_KEY).unwrap().is_none());
    // A template without a name is refused and stays open.
    painter.click_action(&mut app, &ctx, &mut time, "export_template_new");
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "template_save");
    let editor = app.interchange.export.as_ref().unwrap().editor.as_ref();
    assert!(editor.is_some_and(|e| e.error.is_some()));
    // Name it, filter to activities with remaining work, sort by Activity ID descending.
    {
        let e = app
            .interchange
            .export
            .as_mut()
            .unwrap()
            .editor
            .as_mut()
            .unwrap();
        e.draft.name = "Open work".into();
        e.draft.group_by_wbs = false;
        e.draft.filter.push(Condition {
            column: Column::RemainingDuration.key(),
            op: Op::GreaterThan,
            value: "0".into(),
        });
        e.draft.sort.push(SortKey {
            column: Column::ActivityId.key(),
            descending: true,
        });
    }
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "template_save");
    {
        let w = app.interchange.export.as_ref().unwrap();
        assert!(w.editor.is_none());
        assert_eq!(w.template, Some(0));
    }
    // Stored in the workspace preferences, readable by the command line.
    let stored = app.store.preference(et::PREFERENCE_KEY).unwrap().unwrap();
    let (list, rejected) = et::parse_list(&stored);
    assert!(rejected.is_empty());
    assert_eq!(list[0].name, "Open work");
    // Run it from the wizard.
    let file = dir.join("template.xlsx");
    {
        let w = app.interchange.export.as_mut().unwrap();
        w.path = file.display().to_string();
        w.step = 3;
    }
    app.export_finish();
    assert!(app.interchange.export.is_none());
    app.interchange.message = None;
    let wb = xi::read_workbook(&file).unwrap();
    let s = wb.sheet("Activities").unwrap();
    let codes: Vec<String> = wb.sheets[s]
        .rows
        .iter()
        .skip(1)
        .map(|r| r[0].text())
        .collect();
    assert!(!codes.is_empty() && codes.len() < app.project.model.activities.len());
    assert!(codes.windows(2).all(|w| w[0] > w[1]), "{codes:?}");
    // A new wizard offers it again; a damaged entry is rejected, the rest load.
    let mut damaged: Vec<serde_json::Value> = serde_json::from_str(&stored).unwrap();
    damaged.push(serde_json::json!({"name": "Broken", "subject": "Activities"}));
    app.store
        .set_preference(
            et::PREFERENCE_KEY,
            &serde_json::to_string(&damaged).unwrap(),
        )
        .unwrap();
    app.open_export_wizard();
    let w = app.interchange.export.as_ref().unwrap();
    assert_eq!(w.templates.len(), 1);
    assert_eq!(w.template_notes.len(), 1, "{:?}", w.template_notes);
    // Delete removes it from the preferences.
    {
        let w = app.interchange.export.as_mut().unwrap();
        w.format = ExportFormat::Modern;
        w.step = 2;
        w.template = Some(0);
    }
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "export_template_delete");
    let stored = app.store.preference(et::PREFERENCE_KEY).unwrap().unwrap();
    assert_eq!(et::parse_list(&stored).0.len(), 0);
}

#[test]
fn render_template_editor_and_import_preview_with_refused_cells() {
    use openxer_interchange::export_template::{Condition, Op, SortKey};
    let dir = temp("m3-render");
    let (mut app, ctx) = app(&dir);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    app.open_export_wizard();
    {
        let w = app.interchange.export.as_mut().unwrap();
        w.format = ExportFormat::Modern;
        w.step = 2;
    }
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "export_template_new");
    {
        let e = app
            .interchange
            .export
            .as_mut()
            .unwrap()
            .editor
            .as_mut()
            .unwrap();
        e.draft.name = "Open work".into();
        e.draft.filter.push(Condition {
            column: Column::RemainingDuration.key(),
            op: Op::GreaterThan,
            value: "0".into(),
        });
        e.draft.sort.push(SortKey {
            column: Column::Finish.key(),
            descending: false,
        });
        e.pick_chosen = Some(1);
    }
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(out_dir().join("m3-template-editor.png"))
        .unwrap();
    // The preview: one refused cell, one accepted, one out-of-sequence start.
    app.interchange.export = None;
    let p = app.project.clone();
    let behind = p
        .model
        .links
        .iter()
        .find(|l| {
            p.model.activities[l.pred].status == StatusType::NotStarted
                && p.model.activities[l.succ].status == StatusType::NotStarted
                && p.model.activities[l.succ].task_type == openxer_engine::model::TaskType::Task
        })
        .map(|l| p.model.activities[l.succ].task_code.clone())
        .unwrap();
    let start = Cell::Number(
        (p.model.project.data_date.unwrap_or(0) + p.model.origin_serial * DAY) as f64 / DAY as f64,
    );
    let wb = xi::Workbook {
        sheets: vec![xi::Sheet {
            name: "Field updates".into(),
            rows: vec![
                [
                    "Activity ID",
                    "Actual Start",
                    "Remaining Duration (d)",
                    "Duration % Complete",
                ]
                .map(|h| Cell::Text(h.into()))
                .to_vec(),
                vec![Cell::Text(behind), start, Cell::Number(2.), Cell::Empty],
                vec![
                    Cell::Text("RB-1040".into()),
                    Cell::Empty,
                    Cell::Empty,
                    Cell::Number(50.),
                ],
            ],
        }],
    };
    app.open_sheet_import(String::new());
    let lookup = Lookup::new(&app.project);
    let detected = xi::detect(&wb, &lookup).unwrap();
    let mapping = xi::auto_map(&detected, &lookup);
    {
        let w = app.interchange.sheet.as_mut().unwrap();
        w.wb = Some(wb);
        w.detected = Some(detected);
        w.mapping = Some(mapping);
    }
    app.sheet_plan();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out_dir().join("m3-import-preview.png")).unwrap();
    let plan = app
        .interchange
        .sheet
        .as_ref()
        .unwrap()
        .plan
        .as_ref()
        .unwrap();
    assert_eq!(plan.rows[1].kind, xi::RowKind::Error, "{:?}", plan.rows[1]);
    assert!(!plan.notes.is_empty(), "the out-of-sequence start is noted");
}

#[test]
fn xer_import_files_new_projects_in_the_chosen_folder_and_undo_takes_them_back() {
    let dir = temp("eps-folder");
    let (mut app, ctx) = app(&dir);
    let east = {
        let mut ws = app.workspace().clone();
        let id = ws.add_folder(None, "East Region", Some("EAST")).unwrap();
        app.store
            .set_preference(crate::enterprise::workspace::KEY, &ws.to_text())
            .unwrap();
        app.enterprise.cache = Some(ws);
        id
    };
    // The default is the folder selected in the Projects view.
    app.views.projects.selected = None;
    app.views.projects.selected_eps = Some(east);
    app.open_import_wizard();
    assert_eq!(app.interchange.xer.as_ref().unwrap().folder, east);
    app.interchange.xer = None;
    app.views.projects.selected_eps = None;
    app.open_import_wizard();
    let top = app.workspace().eps()[0].id;
    assert_eq!(
        app.interchange.xer.as_ref().unwrap().folder,
        top,
        "else the first top folder"
    );
    // Import a copy as a new project into EAST.
    let file = dir.join("other.xer");
    let mut other = app.project.clone();
    other.model.project.name = "OTHER".into();
    other.updated_document().write(&file).unwrap();
    {
        let w = app.interchange.xer.as_mut().unwrap();
        w.path_text = file.display().to_string();
        w.step = 2;
    }
    app.xer_load_files();
    // The page offers the folder choice.
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    assert!(app.action_rects.contains_key("import_folder"));
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/eps-tracking");
    std::fs::create_dir_all(&out).unwrap();
    image.save(out.join("import-folder-choice.png")).unwrap();
    app.interchange.xer.as_mut().unwrap().folder = east;
    app.interchange.xer.as_mut().unwrap().open_after = false;
    app.xer_prepare();
    app.xer_finish();
    let rows = workspace::rows(&app.store).unwrap();
    let new = rows.iter().find(|r| r.title == "OTHER").unwrap().id;
    assert_eq!(app.workspace().node_of(Some(new)), east);
    // The open project stays where it was.
    let open = app.workspace_row().unwrap();
    assert_eq!(app.workspace().node_of(Some(open)), top);
    // Undo Last Import removes the project and its placement.
    app.undo_last_import();
    assert!(app.store.source(new).is_err());
    assert!(app.workspace().projects_under(east, &[new]).is_empty());
    assert!(app.error.is_none(), "{:?}", app.error);
}

/// Back to the file page and Next again reads the files again but keeps the
/// Import Action and Import To chosen for the same projects, and the
/// spreadsheet wizard keeps the column mapping chosen for the same columns.
#[test]
fn back_then_next_keeps_the_choices_made() {
    let dir = temp("back-next");
    let (mut app, _ctx) = app(&dir);
    let file = dir.join("update.xer");
    app.project.updated_document().write(&file).unwrap();
    let original = workspace::rows(&app.store).unwrap()[0].id;
    app.open_import_wizard();
    app.interchange.xer.as_mut().unwrap().path_text = file.display().to_string();
    app.xer_load_files();
    {
        let w = app.interchange.xer.as_mut().unwrap();
        w.rows[0].action = ImportAction::UpdateExisting;
        w.rows[0].target = Some(original);
    }
    app.xer_load_files();
    let w = app.interchange.xer.as_ref().unwrap();
    assert_eq!(w.rows[0].action, ImportAction::UpdateExisting);
    assert_eq!(w.rows[0].target, Some(original));
    app.interchange.xer = None;

    let sheet = dir.join("layout.xlsx");
    app.export_excel_to(&sheet);
    app.open_sheet_import(sheet.display().to_string());
    app.sheet_load();
    let auto = app
        .interchange
        .sheet
        .as_ref()
        .unwrap()
        .mapping
        .clone()
        .unwrap();
    let mut chosen = auto.clone();
    let k = chosen
        .activity
        .iter()
        .rposition(|t| *t != xi::Target::Ignore)
        .unwrap();
    chosen.activity[k] = xi::Target::Ignore;
    assert_ne!(chosen, auto);
    app.interchange.sheet.as_mut().unwrap().mapping = Some(chosen.clone());
    app.sheet_load();
    let w = app.interchange.sheet.as_ref().unwrap();
    assert!(w.error.is_none(), "{:?}", w.error);
    assert_eq!(w.mapping.as_ref(), Some(&chosen));
}
