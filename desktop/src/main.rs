#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// New ids are handed out by explicit counters that read more plainly than a
// zipped range in the paste and copy loops.
#![allow(clippy::explicit_counter_loop)]
mod activities;
mod activity_delete;
mod activity_wizard;
mod admin_prefs;
mod ai_agents;
mod available_options;
mod baselines;
mod busy;
mod calendars;
mod chat;
mod chrome;
mod close_guard;
mod compare;
mod context_menu;
mod crash;
mod critical_path;
mod defaults;
mod details;
mod diag_log;
mod dialog_keys;
mod dialog_kit;
mod dialogs;
mod dictionaries;
mod dockbars;
mod dockbars_ui;
mod editing;
mod enterprise;
mod estimation;
mod external_apps;
mod first_run;
mod format;
mod frame;
mod gantt;
mod gantt_geometry;
mod grid;
#[cfg(test)]
mod h6_renders;
mod help;
mod history;
mod hscroll;
mod instance;
mod interchange;
mod kit_window;
#[cfg(test)]
mod l1_renders;
#[cfg(test)]
mod l2_renders;
mod layout_opts;
mod loops;
#[cfg(test)]
mod m6_renders;
mod markdown;
mod memo;
mod menu;
mod menu_keys;
mod new_project;
mod open_set;
mod panes;
mod print;
mod project_io;
#[cfg(test)]
mod readme_hero;
mod records;
mod relationships;
mod reports;
mod resource_wizard;
mod schedule_check;
mod schedule_dialogs;
mod scheduling;
mod session_history;
mod share_dictionaries;
mod spell;
mod start_hold;
mod store;
mod tab_strip;
mod theme;
mod timeline;
mod timescale;
mod toolbars;
#[cfg(test)]
mod ui_batch1_renders;
#[cfg(test)]
mod ui_batch3_renders;
mod user_prefs;
mod util;
mod view;
mod views;
mod window_place;
mod workspace_sync;
use chrome::{BLUE, CHROME, CRITICAL, FONT_SIZE, INK, SELECT};
use details::DetailTab;
use dialogs::LinkEditor;
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Key, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};
use gantt::{BarGeometry, LinkDrag, RowAnchor};
use openxer_engine::{calendar::DAY, model::*, options::*, scheduler};
use openxer_xer::{constraint_label, date_string, status_label, type_label, Document, ProjectData};
use relationships::Edge;
use scheduling::schedule_model;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Instant,
};
use store::Store;
use util::{displayed_finish, displayed_start, drag_motion, segment_distance};
struct App {
    project: ProjectData,
    store: Store,
    saved_projects: Vec<(i64, String)>,
    selected: Option<usize>,
    detail: details::DetailState,
    records_revision: u64,
    records_serial: u64,
    query: String,
    wbs_filter: Option<i64>,
    collapsed: BTreeSet<String>,
    dirty: bool,
    status: String,
    error: Option<String>,
    show_schedule: bool,
    show_log: bool,
    show_about: bool,
    data_date: String,
    must_finish: String,
    log: Vec<String>,
    pixels_per_day: f32,
    timeline_origin: i64,
    pending: Option<(Document, String)>,
    /// A project read from a file the workspace already has a copy of (the
    /// copy's workspace id), waiting for the user to choose.
    replace_prompt: Option<(ProjectData, i64)>,
    date_edits: BTreeMap<String, String>,
    grid_fraction: f32,
    chart_width: f32,
    edit_cell: Option<(usize, String)>,
    grid: grid::GridState,
    edit_text: String,
    edit_focus: bool,
    draft_options: Option<ScheduleOptions>,
    /// `--capture-dir`: a debug build's folder for frame screenshots.
    #[cfg(debug_assertions)]
    capture_dir: Option<PathBuf>,
    #[cfg(debug_assertions)]
    capture_count: usize,
    frame_number: usize,
    history: history::History,
    show_details: bool,
    link_drag: Option<LinkDrag>,
    link_editor: Option<LinkEditor>,
    scroll_to_selected: bool,
    schedule_runs: usize,
    action_rects: BTreeMap<String, [f32; 4]>,
    /// Shift+F10 and the Menu key: the request to open the focused view's
    /// context menu (see `context_menu`).
    ctx_menu: context_menu::KeyState,
    bar_geometry: Vec<BarGeometry>,
    /// Every chart row drawn in the last frame, by what a text attachment
    /// can hang from (an activity or a grouping band).
    row_geometry: Vec<(RowAnchor, Rect)>,
    link_geometry: Vec<(usize, Vec<Pos2>)>,
    gantt_left: f32,
    view: view::ViewSettings,
    gantt: gantt::settings::GanttSettings,
    gantt_ui: gantt::GanttUi,
    /// The help window (see `help`).
    help: help::HelpState,
    loops: loops::State,
    view_snapshot: String,
    view_save_at: Option<f64>,
    sched: schedule_dialogs::ScheduleDialogs,
    views: views::Views,
    calendars: Option<calendars::CalendarsState>,
    dictionaries: dictionaries::DictionaryState,
    interchange: interchange::Interchange,
    formats: format::Formats,
    prefs: user_prefs::PrefsState,
    editing: editing::EditingState,
    /// Activities waiting for the delete confirmation.
    delete_confirm: Option<Vec<i64>>,
    baselines: baselines::BaselineState,
    /// Top and bottom pane choices of the Activities view.
    panes: panes::PanesState,
    /// Page Setup, Print Preview and PDF output.
    print: print::PrintState,
    /// The other open projects, the Default Project and the undo journal.
    open_set: open_set::OpenSet,
    /// Tools > Schedule Comparison.
    compare: compare::CompareState,
    /// Project Checker, Report Wizard and saved reports.
    reports: reports::ReportsState,
    /// Edit > Spell Check, while open.
    spell: Option<spell::SpellState>,
    /// Tools > Top Down Estimation, while open.
    estimation: Option<estimation::TopDownDialog>,
    /// Line numbers, spotlight, fonts, reorganization and their dialogs.
    layout_opts: layout_opts::LayoutOpts,
    /// EPS, OBS, roles, curves, shifts, currencies, funding, financial
    /// periods and portfolios.
    enterprise: enterprise::EnterpriseState,
    /// View > Activity Critical Path.
    critical_path: critical_path::CriticalPathState,
    /// How Available Options lists are shown (Group and Sort By).
    available_view: available_options::AvailableView,
    /// Enterprise > External Applications and View > Launch.
    external: external_apps::State,
    /// File > New: the new-project wizard, while open.
    new_project: Option<new_project::Dialog>,
    /// Edit > Add: the New Activity wizard and its on/off switch.
    activity_wizard: activity_wizard::WizardState,
    /// The AI Agents menu's dialogs and status bar hint.
    ai_agents: ai_agents::AgentsState,
    /// AI Agents > Chat: the window, its agent backend and conversation.
    chat: chat::ChatState,
    /// AI Agents > Skills and the skill editor.
    skills: ai_agents::skills_ui::SkillsState,
    /// The shared workspace dictionary: live sync and its undo steps.
    dict_sync: workspace_sync::DictSync,
    /// Admin > Share Dictionaries Across Projects, while open.
    share_dictionaries: Option<share_dictionaries::ShareDialog>,
    /// A close stopped by a failed save, and what the user chose.
    close_guard: close_guard::CloseGuard,
    /// Background scheduling/leveling worker and its modal progress dialog.
    busy: busy::BusyState,
    /// DWM cloak keeping startup invisible until layout, maximize and first swap settle.
    startup_cloak: window_place::StartupCloak,
}
impl App {
    fn new(context: &egui::Context, store: Store, project: ProjectData) -> Self {
        chrome::configure(context);
        let saved_projects = store.list().unwrap_or_default();
        let data_date = date_string(project.model.origin_serial, project.model.project.data_date);
        let must_finish = date_string(project.model.origin_serial, project.model.project.plan_end);
        let timeline_origin =
            project.model.project.data_date.unwrap_or(0).div_euclid(DAY) * DAY - 2 * DAY;
        let history = history::History::new(&project);
        let mut app = Self {
            project,
            store,
            saved_projects,
            selected: Some(0),
            detail: details::DetailState::default(),
            records_revision: 0,
            records_serial: 0,
            query: String::new(),
            wbs_filter: None,
            collapsed: BTreeSet::new(),
            dirty: false,
            status: "Ready".into(),
            error: None,
            show_schedule: false,
            show_log: false,
            show_about: false,
            data_date,
            must_finish,
            log: vec![],
            pixels_per_day: 16.,
            timeline_origin,
            pending: None,
            replace_prompt: None,
            date_edits: BTreeMap::new(),
            grid_fraction: 0.60,
            chart_width: 520.,
            edit_cell: None,
            grid: grid::GridState::default(),
            edit_text: String::new(),
            edit_focus: false,
            draft_options: None,
            #[cfg(debug_assertions)]
            capture_dir: None,
            #[cfg(debug_assertions)]
            capture_count: 0,
            frame_number: 0,
            history,
            show_details: true,
            link_drag: None,
            link_editor: None,
            scroll_to_selected: false,
            schedule_runs: 0,
            action_rects: BTreeMap::new(),
            ctx_menu: Default::default(),
            bar_geometry: vec![],
            row_geometry: vec![],
            link_geometry: vec![],
            gantt_left: 0.,
            view: view::ViewSettings::default(),
            gantt: Default::default(),
            gantt_ui: Default::default(),
            help: Default::default(),
            loops: Default::default(),
            view_snapshot: String::new(),
            view_save_at: None,
            sched: Default::default(),
            views: views::Views::default(),
            calendars: None,
            dictionaries: Default::default(),
            interchange: Default::default(),
            formats: Default::default(),
            prefs: Default::default(),
            editing: Default::default(),
            delete_confirm: None,
            baselines: Default::default(),
            panes: Default::default(),
            print: Default::default(),
            open_set: Default::default(),
            compare: Default::default(),
            reports: Default::default(),
            spell: None,
            estimation: None,
            layout_opts: Default::default(),
            enterprise: Default::default(),
            critical_path: Default::default(),
            available_view: Default::default(),
            external: Default::default(),
            new_project: None,
            activity_wizard: Default::default(),
            ai_agents: Default::default(),
            chat: Default::default(),
            skills: Default::default(),
            dict_sync: Default::default(),
            share_dictionaries: None,
            close_guard: Default::default(),
            busy: Default::default(),
            startup_cloak: Default::default(),
        };
        app.open_set.active = app
            .store
            .id_of(&app.project.source_name, app.project.model.project.proj_id);
        app.fit();
        app.restore_view();
        app.restore_gantt();
        app.restore_layout();
        app.restore_detail_layout();
        app.restore_baseline_prefs();
        app.restore_grid_columns();
        app.restore_available_view();
        app.restore_views();
        app.restore_formats();
        app.restore_editing();
        app.restore_start_hold();
        app.restore_assistance();
        app.restore_panes();
        app.restore_external_apps();
        app.restore_agent_hint();
        app.restore_chat();
        app
    }
    fn error(&mut self, e: String) {
        self.error = Some(e.clone());
        self.status = e;
    }
    pub(crate) fn diag_log(&self) -> diag_log::DiagLog {
        diag_log::DiagLog::for_store(&self.store.path)
    }
    fn action(&mut self, name: &str, response: &egui::Response) {
        let r = response.rect;
        self.action_rects
            .insert(name.into(), [r.left(), r.top(), r.right(), r.bottom()]);
    }
}
impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Match OpenXER's light chrome so OpenGL never flashes dark/black
        // during startup or resize swaps.
        CHROME.to_normalized_gamma_f32()
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let settling = self.frame_number < 2;
        if !self.startup_cloak.active() {
            window_place::track(ctx);
        }
        self.update_ui(ctx);
        // Settle dockbar button widths and panel sizes during the hidden first
        // frame so the window appears cleanly without multi-frame layout jumps.
        if settling {
            ctx.request_discard("settle initial layout");
        } else {
            self.startup_cloak.step(ctx, true);
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.startup_cloak.uncloak();
        // Chat's conversation archives and ends and its files go (its own only;
        // the panic hook clears every folder this process made).
        self.close_chat();
        window_place::save(&self.store);
        self.with_first_activities_state(|app| {
            app.save_view();
            app.save_layout();
        });
        self.save_views();
        // Close Without Saving (or a close that skipped the question) still
        // tries once more; if that fails the changes are kept as a recovery
        // copy, offered at the next start.
        if self.any_dirty() && self.try_save().is_err() {
            let _ = self.write_recovery_copies_now();
        }
    }
}
/// `--smoke-test`: a debug build's self-check of scheduling, saving and the
/// XER round trip, without a window.
#[cfg(debug_assertions)]
fn smoke_test(path: &Path) -> Result<(), String> {
    let mut p = openxer_xer::starter();
    p.model.validate()?;
    schedule_model(&mut p.model, &p.resources)?;
    if p.model
        .activities
        .iter()
        .any(|a| a.early_start.is_none() || a.early_finish.is_none())
    {
        return Err("Starter schedule left an activity undated".into());
    }
    p.details.get_mut(&1010).unwrap().name = "Verified activity edit".into();
    let mut store = Store::open(path.into())?;
    store.save(&p)?;
    drop(store);
    let store = Store::open(path.into())?;
    let q = store.last()?.ok_or("Saved project missing")?;
    if q.details[&1010].name != "Verified activity edit" {
        return Err("Saved activity edit did not persist".into());
    }
    let exported = q.updated_document().text();
    let roundtrip = ProjectData::from_document(Document::parse(&exported)?, 1, "smoke".into())?;
    if roundtrip.model.activities.len() != 14 || roundtrip.model.links.len() != 16 {
        return Err("XER round trip changed network size".into());
    }
    println!("PASS: native Rust scheduling, activity edit, SQLite reopen and XER round trip (14 activities, 16 links)");
    Ok(())
}
/// Add `error` to the errors shown when the window opens.
fn note(errors: &mut Option<String>, error: String) {
    *errors = Some(match errors.take() {
        Some(earlier) => format!("{earlier}\n{error}"),
        None => error,
    });
}
fn main() -> eframe::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // A panic tells the user what happened instead of the window vanishing.
    crash::install(&args);
    #[cfg(debug_assertions)]
    if args.first().is_some_and(|a| a == "--smoke-test") {
        let path = args
            .get(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("openxer-smoke.db"));
        match smoke_test(&path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    // The headless print mode `openxer-cli export-pdf` runs: it reads the
    // summary and the error from the pipes it gives this process, so these
    // prints reach it even from a release build.
    if args.first().is_some_and(|a| a == "export-pdf") {
        let rest: Vec<String> = args[1..]
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        match print::cli::run(&rest) {
            Ok(summary) => {
                println!("{summary}");
                return Ok(());
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    // Setup and the uninstaller look for this to ask the user to close
    // OpenXER before they replace or remove its files.
    instance::hold_app_mutex();
    chat::live::work::remove_stale(&std::env::temp_dir());
    let Some(Startup {
        lock,
        store,
        project,
        pending,
        replace_prompt,
        error: initial_error,
    }) = launch(&args, show_fatal)
    else {
        std::process::exit(1)
    };
    chrome::preload_fonts();
    let initial_title = format!(
        "OpenXER : {} ({})",
        project.model.project.name,
        views::project_title(&project)
    );
    let saved_place = window_place::load(&store);
    window_place::seed(saved_place);
    // On Windows, winit 0.30 calls ShowWindow(SW_MAXIMIZE) -> ShowWindow(SW_HIDE)
    // during CreateWindowExW when maximized is set, causing visible startup flashes.
    // Instead, create hidden non-maximized, cloak in DWM, maximize behind the cloak,
    // and reveal once the final maximized OpenGL frame has swapped.
    let defer_maximize = cfg!(target_os = "windows") && saved_place.is_some_and(|p| p.maximized);
    let mut viewport = window_place::viewport(saved_place).with_title(initial_title);
    if defer_maximize {
        viewport = viewport.with_maximized(false);
    }
    let options = eframe::NativeOptions {
        viewport,
        persist_window: false,
        ..Default::default()
    };
    let result = eframe::run_native(
        "OpenXER",
        options,
        Box::new(move |cc| {
            let cloak = window_place::StartupCloak::engage(defer_maximize);
            let mut app = App::new(&cc.egui_ctx, store, project);
            app.startup_cloak = cloak;
            app.adopt_file_baselines();
            app.restore_open_set();
            #[cfg(debug_assertions)]
            {
                app.capture_dir = args
                    .windows(2)
                    .find(|a| a[0] == "--capture-dir")
                    .map(|a| PathBuf::from(&a[1]));
            }
            app.pending = pending;
            app.replace_prompt = replace_prompt;
            app.note_loops_on_open();
            app.note_dictionary_sharing();
            if let Some(error) = initial_error.filter(|_| !app.loops.prompt) {
                app.error(error);
            }
            Ok(Box::new(app))
        }),
    );
    // The workspace stays locked until the app has saved on exit.
    drop(lock);
    // A window that could not be created (no usable graphics driver, for
    // example) is reported like any other startup failure.
    if let Err(error) = &result {
        show_fatal(&format!(
            "OpenXER could not open its window:
{error}"
        ));
    }
    result
}

/// Tell the user, in a message box, why OpenXER cannot start. Release builds
/// have no console, so this is the only place they would see it.
fn show_fatal(message: &str) {
    eprintln!("{message}");
    crash::native_error_box("OpenXER cannot start", message);
}

/// What the window opens with.
struct Startup {
    /// Held while the app runs: no second app opens the workspace.
    lock: store::WorkspaceLock,
    store: Store,
    project: ProjectData,
    pending: Option<(Document, String)>,
    /// A project read from a file the workspace already has a copy of (the
    /// copy's workspace id), waiting for the user to choose.
    replace_prompt: Option<(ProjectData, i64)>,
    /// Errors to show when the window opens.
    error: Option<String>,
}

/// Open the workspace and the project the window starts on, from the
/// command line `args`. When the workspace cannot be opened (read-only,
/// locked, damaged, made by a newer OpenXER) `fatal` is told what is wrong
/// and where the file is, and there is nothing to start: the app shows it in
/// a message box, tests record it.
fn launch(args: &[std::ffi::OsString], fatal: impl FnOnce(&str)) -> Option<Startup> {
    let db = args
        .windows(2)
        .find(|a| a[0] == "--database")
        .map(|a| PathBuf::from(&a[1]))
        .unwrap_or_else(Store::default_path);
    let file = instance::file_to_open(args);
    let locked = match store::WorkspaceLock::take(&db) {
        // OpenXER is already running on this workspace (a second
        // double-click on an XER file, for example): say how to open the
        // file there rather than only that the workspace is busy.
        Err(e) if e.contains("already open in another OpenXER window") => {
            let open = file
                .as_ref()
                .map(|f| {
                    format!(
                        "\n\nTo open {}, switch to the OpenXER window that is already open and use File > Import... there (or drop the file on that window).",
                        f.display()
                    )
                })
                .unwrap_or_default();
            fatal(&format!(
                "OpenXER is already running.\n\nIts workspace {} is already open in another OpenXER window.{open}",
                db.display()
            ));
            return None;
        }
        other => other,
    };
    let opened = locked.and_then(|lock| Store::open(db.clone()).map(|store| (lock, store)));
    let (lock, mut store) = match opened {
        Ok(opened) => opened,
        Err(e) => {
            fatal(&format!(
                "OpenXER cannot open its workspace:\n{}\n\n{e}\n\nNothing was changed. Close any other program using the file, check that it can be written, or start OpenXER with --database and another workspace file.",
                db.display()
            ));
            return None;
        }
    };
    let mut initial_error = None;
    let mut pending = None;
    let mut replace_prompt = None;
    // The as-planned copy of the sample, while the window starts on a fresh
    // sample (it becomes its baseline once the sample is stored).
    let mut fresh_sample = None;
    let mut project = match store.last() {
        Ok(Some(saved)) => saved,
        Ok(None) => {
            let (current, plan) = fallback_sample(&store);
            fresh_sample = Some(plan);
            current
        }
        Err(error) => {
            initial_error = Some(format!(
                "The project open when OpenXER last closed could not be opened, so the sample project is shown instead. That project is still in the workspace: {error}"
            ));
            let (current, plan) = fallback_sample(&store);
            fresh_sample = Some(plan);
            current
        }
    };
    if let Some(path) = file {
        match Document::read(&path) {
            Ok(doc) => {
                let projects = openxer_interchange::baseline::schedule_projects(&doc);
                let requested = args
                    .windows(2)
                    .find(|a| a[0] == "--project")
                    .and_then(|a| a[1].to_str())
                    .and_then(|s| s.parse::<i64>().ok());
                let id = requested.or_else(|| (projects.len() == 1).then(|| projects[0].0));
                if let Some(id) = id {
                    match ProjectData::from_document(doc, id, path.display().to_string()) {
                        Ok(imported) => {
                            let key =
                                (imported.source_name.clone(), imported.model.project.proj_id);
                            if let Some(row) = store.id_of(&key.0, key.1) {
                                // The workspace has a copy: the user chooses
                                // once the window is open.
                                replace_prompt = Some((imported, row));
                            } else {
                                // A file, even with a workspace project's name.
                                store.forget_mark(&key);
                                project = imported;
                                fresh_sample = None;
                            }
                        }
                        Err(error) => initial_error = Some(error),
                    }
                } else if projects.is_empty() {
                    initial_error = Some("No projects found in the XER file".into());
                } else {
                    pending = Some((doc, path.display().to_string()));
                }
            }
            Err(error) => initial_error = Some(error),
        }
    }
    if project
        .model
        .activities
        .iter()
        .all(|a| a.early_start.is_none())
    {
        if let Err(error) = schedule_model(&mut project.model, &project.resources) {
            initial_error = Some(error);
        }
    }
    // The dictionary before the sample joins it: what the sample adds is
    // the sample's own, and goes when the sample is removed.
    let dictionary_before = fresh_sample
        .as_ref()
        .and_then(|_| store.dictionary().ok().map(|(d, _)| d));
    // The project starts as the workspace stores it; a failed save is shown
    // when the window opens rather than lost.
    match store.save_stored(&project) {
        Ok((_, Some(doc))) => {
            if let Err(error) = store::adopt_document(&mut project, doc) {
                note(&mut initial_error, error);
            }
        }
        Ok(_) => {}
        Err(error) => note(
            &mut initial_error,
            format!("The project could not be saved to the workspace: {error}"),
        ),
    }
    if let (Some(plan), Some(before)) = (fresh_sample, dictionary_before) {
        if let Err(error) = first_run::install_sample(&mut store, &mut project, &plan, &before) {
            note(
                &mut initial_error,
                format!("The sample project's baseline could not be stored: {error}"),
            );
        }
    }
    Some(Startup {
        lock,
        store,
        project,
        pending,
        replace_prompt,
        error: initial_error,
    })
}

/// The sample project (as of its data date, and as planned), shown when the
/// workspace has no project to open. If the workspace already has a project
/// under the sample's name and key (a schedule built in the sample, which
/// may have failed to load), the sample gets a name of its own, so saving it
/// can never replace that project.
fn fallback_sample(store: &Store) -> (ProjectData, ProjectData) {
    let sample = openxer_xer::sample_project::sample_project();
    let mut p = sample.current;
    let key = p.model.project.proj_id;
    if store.id_of(&p.source_name, key).is_some() {
        let base = format!(
            "{} {}",
            p.source_name,
            chrono::Local::now().format("%Y-%m-%d %H.%M.%S")
        );
        let mut name = base.clone();
        let mut n = 2;
        while store.id_of(&name, key).is_some() {
            name = format!("{base} ({n})");
            n += 1;
        }
        p.source_name = name;
    }
    (p, sample.plan)
}

#[cfg(test)]
mod agent_coverage;
#[cfg(test)]
mod dialog_renders;
#[cfg(test)]
mod dialog_sweep;
#[cfg(test)]
mod dockbars_tests;
#[cfg(test)]
mod float_path_end_tests;
#[cfg(test)]
mod headless_render;
#[cfg(test)]
mod headless_ui;
#[cfg(test)]
mod hscroll_tests;
#[cfg(test)]
mod install_tests;
#[cfg(test)]
mod new_project_wbs_tests;
#[cfg(test)]
mod perf;
#[cfg(test)]
mod polish_tests;
#[cfg(test)]
mod qa_sweep;
#[cfg(test)]
mod qa_workflows;
#[cfg(test)]
mod sample_renders;
#[cfg(test)]
mod scheduling_guards;
#[cfg(test)]
mod splitter_tests;
#[cfg(test)]
mod startup_tests;
#[cfg(test)]
mod test_corpus;
#[cfg(test)]
mod timeline_tests;
#[cfg(test)]
mod workspace_safety_tests;
#[cfg(test)]
mod workspace_sync_tests;
#[cfg(test)]
mod write_safety_tests;
