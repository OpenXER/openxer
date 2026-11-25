//! End-to-end QA sweep through the production UI.
//!
//! A `Driver` runs `App::update_ui` with synthetic egui input only: no native
//! window, no system pointer. The sweep imports a project through File >
//! Import, visits every view, bottom pane and details tab, opens every menu,
//! submenu and toolbar command and closes whatever dialog it opened, runs F9,
//! leveling and Check Schedule through their dialogs, edits the project in the
//! grid, the details tabs, by relationship drag and by assignment, copies and
//! pastes, undoes and redoes everything, saves, reopens, exports XER and both
//! spreadsheet layouts through the Export wizard and reads them back.
//!
//! The starter tests (`starter_project_survives_the_full_ui_sweep` and its
//! `_menus_N` slices) always run. The corpus tests
//! (`corpus_projects_survive_the_full_ui_sweep_NN`) run every XER in the
//! `OPENXER_CORPUS` folder (skipped with a printed reason when the variable
//! is not set or the folder is absent). The sweep is split so the test
//! runner can run the parts side by side: each project's menu and toolbar
//! commands go to `MENU_SLICES` slices, each run on a fresh import after
//! the views sweep, and the rest of the scenario runs on its own fresh
//! import; the corpus parts are shared out over `CORPUS_SHARDS` tests. The
//! filter `full_ui_sweep` selects every part. Debug builds skip
//! files over 200 KB (5640, 369 and one more) unless
//! `OPENXER_QA_LARGE=1`, because frames of large projects cost tens of
//! milliseconds unoptimized and the run would take many minutes. For the
//! same reason a debug run gives the menu and toolbar sweep (about 190
//! commands and 70 buttons) to the first corpus project only and runs the
//! rest of the scenario on every project; `OPENXER_QA_FULL=1` or a release
//! run (`cargo.exe test -p openxer-desktop --release qa_sweep`) sweeps the
//! menus on every project, 369 included. `OPENXER_QA_ONLY=5640,6031` limits
//! the corpus run to those files and `OPENXER_QA_NO_MENUS=1` skips the menu
//! and toolbar sweep (both for quick reruns; pass them to `cargo.exe` with
//! `WSLENV` from WSL).
use super::*;
use crate::grid::Row;
use std::collections::BTreeSet;

pub(crate) struct Driver {
    pub app: App,
    pub ctx: egui::Context,
    pub time: f64,
    pub dir: PathBuf,
    /// Findings that are not failures (skipped steps, expected refusals).
    pub notes: Vec<String>,
    /// Logical screen size; 1500 x 900 unless a test sets another.
    pub screen: Vec2,
    /// The dialog formatting probe (off unless a survey enables it).
    pub probe: crate::dialog_sweep::Probe,
    /// The last `try_command` opened a dialog.
    pub opened_dialog: bool,
    /// The project the driver started with, for [`Driver::reset_app`].
    pub origin: Option<ProjectData>,
    /// Press Enter on every dialog a sweep opens (survey drivers only: the
    /// default button may change the project, so the app starts over after).
    pub enter_check: bool,
    /// Brings a fresh app to the state the sweep starts from.
    pub setup: Option<fn(&mut Driver)>,
    /// The menu and toolbar sweeps try only the commands of this slice
    /// (index, number of slices; see [`in_slice`]). None: every command.
    pub slice: Option<(usize, usize)>,
}

const SCREEN: Vec2 = Vec2::new(1500., 900.);

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

/// Actions that must never be clicked by a generic sweep: they open a native
/// file dialog, close the application, switch or create workspace projects,
/// or change the project without a dialog (those are exercised explicitly).
fn never_click(action: &str) -> bool {
    matches!(
        action,
        "menu_open"
            | "open"
            | "menu_exit"
            | "view_close"
            | "menu_add"
            | "add"
            | "edit_cut"
            | "edit_fill_down"
            | "edit_link"
            | "edit_unlink"
            | "dock_cut"
            | "dock_delete"
            | "dock_paste"
            | "file_import"
            | "file_export"
            | "undo"
            | "redo"
            | "menu_reset_toolbars"
            | "menu_reset_toolbar_layout"
    ) || action.starts_with("menu_recent_")
        || action.starts_with("menu_toolbar_")
        || action.starts_with("bar_grip_")
        || action.starts_with("bar_chevron_")
        || action.starts_with("chevron_")
        || action.starts_with("float_")
        || action.starts_with("menu_launch_")
        || action.starts_with("menu_columns_set_")
        || action.starts_with("menu_group_set_")
        || action.starts_with("dock_move_")
        || action.starts_with("menu_bar_")
        || action.starts_with("dock_tab")
}

/// Default buttons that act without closing their dialog.
const ENTER_KEEPS_OPEN: &[&str] = &[
    "Find Next",
    "Apply",
    "Replace",
    "Replace All",
    "Next",
    "Ignore",
];

/// Commands whose window is a document viewer with its own keys, not a
/// dialog with a default button.
// The help window leaves Enter to its search field (no default button).
const ENTER_VIEWERS: &[&str] = &[
    // Chat's Enter sends from its message box; with nothing typed it waits.
    crate::chat::COMMAND,
    crate::chat::ASK_SELECTION_COMMAND,
    "menu_print_preview",
    "tool_print_preview",
    "menu_local_help",
    "menu_context_help",
    "tool_local_help",
    "menu_whats_new",
    "tool_whats_new",
];

/// Rendered controls that close a dialog, most cautious first.
fn closer_rank(action: &str) -> Option<usize> {
    // A view's command bar (`<view>_cmd_<name>`) is not a dialog.
    if action.contains("_cmd_") {
        return None;
    }
    ["_cancel", "_no", "_close", "title_close", "_ok"]
        .iter()
        .position(|suffix| action.ends_with(suffix))
}

/// Everything about the window that a cancelled dialog must leave alone.
#[derive(Debug, PartialEq)]
struct ViewState {
    main: views::MainView,
    tab_groups: usize,
    toolbars_locked: bool,
    top: panes::TopView,
    bottom: panes::BottomView,
    details: bool,
    relationships: bool,
    progress_line: bool,
    status_bar: bool,
    filter_active: bool,
    collapsed: BTreeSet<String>,
    line_numbers: bool,
    hint_help: bool,
    spotlight: bool,
    reorganize_off: bool,
    critical_paths: bool,
    path_count: usize,
    path_group: bool,
    placing_text: bool,
}

/// The menu bar: each menu and its submenus, as the sweeps visit them.
pub(crate) const MENUS: [(&str, &[&str]); 9] = [
    ("file", &[]),
    ("edit", &["Assign"]),
    (
        "view",
        &[
            "Layout",
            "Show on Top",
            "Show on Bottom",
            "Columns",
            "Filter By",
            "Group and Sort by",
            "Check Schedule",
            "Attachments",
            "Activity Critical Path",
            "Zoom",
            "Toolbars",
            "Tab Groups",
            "Run Program",
        ],
    ),
    ("project", &[]),
    ("enterprise", &[]),
    ("tools", &["Reports"]),
    ("ai_agents", &[]),
    ("admin", &[]),
    ("help", &[]),
];

impl Driver {
    pub fn new(name: &str, project: ProjectData) -> Self {
        Self::build(name, project, SCREEN, Default::default())
    }

    /// A driver whose probe is on from the first frame, on a screen of the
    /// given size.
    pub fn probed(name: &str, project: ProjectData, screen: Vec2, render: Option<PathBuf>) -> Self {
        let probe = crate::dialog_sweep::Probe::on(render);
        Self::build(name, project, screen, probe)
    }

    fn build(
        name: &str,
        project: ProjectData,
        screen: Vec2,
        probe: crate::dialog_sweep::Probe,
    ) -> Self {
        // Drivers on the same project start side by side in the split
        // sweeps, so the clock alone could give two of them one folder.
        static BUILT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let folder = format!("openxer-qa-{name}-{pid}-{unique}-{serial}");
        let dir = std::env::temp_dir().join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.join("workspace.db")).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let origin = project.clone();
        let mut app = App::new(&ctx, store, project);
        app.save();
        // The AI Agents menu looks in an empty profile in the temporary
        // folder, never at the real client configs.
        app.ai_agents.env = Some(crate::ai_agents::temp_profile(&dir));
        // Reports and logs go to the temporary folder, never to Documents.
        app.sched.check_report_path = dir.join("check.html").display().to_string();
        app.sched.log_path = dir.join("SCHEDLOG.TXT").display().to_string();
        let mut d = Self {
            app,
            ctx,
            time: 0.,
            dir,
            notes: vec![],
            screen,
            probe,
            opened_dialog: false,
            origin: Some(origin),
            enter_check: false,
            setup: None,
            slice: None,
        };
        d.frames(3);
        d
    }

    pub fn frame_with(
        &mut self,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        self.time += 0.1;
        let app = &mut self.app;
        let probing = self.probe.enabled;
        let keep = self.probe.render.is_some();
        let mut seen = None;
        let output = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.screen)),
                time: Some(self.time),
                events,
                modifiers,
                focused: true,
                ..Default::default()
            },
            |ctx| {
                app.update_ui(ctx);
                if probing {
                    let actions: Vec<(String, Rect)> = app
                        .action_rects
                        .iter()
                        .map(|(k, r)| {
                            (
                                k.clone(),
                                Rect::from_min_max(Pos2::new(r[0], r[1]), Pos2::new(r[2], r[3])),
                            )
                        })
                        .collect();
                    seen = Some(crate::dialog_sweep::capture(ctx, keep, &actions));
                }
            },
        );
        if let Some(seen) = seen {
            self.probe.frame_done(seen, &output);
        }
        output
    }

    pub fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.frame_with(events, Default::default())
    }

    pub fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let _ = self.frame(vec![]);
        }
    }

    /// Pump real UI frames until any background scheduling or leveling worker
    /// finishes (exercising the live progress dialog on every frame).
    pub fn wait_busy(&mut self) {
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        while self.app.busy_active() {
            assert!(
                Instant::now() < deadline,
                "background scheduling/leveling worker timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
            let _ = self.frame(vec![]);
        }
    }

    pub fn has(&self, action: &str) -> bool {
        self.app.action_rects.contains_key(action)
    }

    #[track_caller]
    pub fn rect(&self, action: &str) -> Rect {
        let at = std::panic::Location::caller();
        let [l, t, r, b] = *self.app.action_rects.get(action).unwrap_or_else(|| {
            panic!(
                "No rendered control {action:?} (asked at {at}); have {:?}",
                self.app.action_rects.keys().collect::<Vec<_>>()
            )
        });
        Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
    }

    pub fn press_at(&mut self, pos: Pos2, button: egui::PointerButton, modifiers: egui::Modifiers) {
        for pressed in [true, false] {
            let _ = self.frame_with(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers,
                    },
                ],
                modifiers,
            );
        }
        let _ = self.frame(vec![]);
    }

    pub fn click_at(&mut self, pos: Pos2) {
        self.press_at(pos, egui::PointerButton::Primary, Default::default());
    }

    /// Click a rendered control by its action name. A window opened last
    /// frame settles its position over two frames first.
    #[track_caller]
    pub fn click(&mut self, action: &str) {
        self.frames(2);
        if !self.has(action) {
            if let Some(menu) = menu::menu_holding(action) {
                self.click(&format!("menu_bar_{}", menu.to_lowercase()));
            }
        }
        let pos = self.rect(action).center();
        self.click_at(pos);
        let _ = self.frame(vec![egui::Event::PointerGone]);
        self.frames(1);
        self.wait_busy();
    }

    pub fn double_click_at(&mut self, pos: Pos2) {
        // Let the double-click window of any earlier click lapse, so the
        // first click here is not read as the second click of a pair.
        self.frames(5);
        let _ = self.frame(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false, true, false] {
            let _ = self.frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }]);
        }
        self.frames(1);
    }

    pub fn key_with(&mut self, key: Key, modifiers: egui::Modifiers) {
        for pressed in [true, false] {
            let _ = self.frame_with(
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
            );
        }
        self.frames(1);
        self.wait_busy();
    }

    pub fn key(&mut self, key: Key) {
        self.key_with(key, Default::default());
    }

    pub fn type_text(&mut self, text: &str) {
        let _ = self.frame(vec![egui::Event::Text(text.into())]);
    }

    /// Visible text shapes of the next frame, with their screen rectangles.
    pub fn texts(&mut self) -> Vec<(String, Rect)> {
        let output = self.frame(vec![]);
        output
            .shapes
            .iter()
            .filter_map(|clipped| {
                if let egui::Shape::Text(shape) = &clipped.shape {
                    let rect = shape.visual_bounding_rect().intersect(clipped.clip_rect);
                    if rect.is_positive() {
                        return Some((shape.galley.text().to_string(), rect));
                    }
                }
                None
            })
            .collect()
    }

    /// Move the pointer over visible text (the last painted copy, which is the
    /// topmost one). False when the text is not on screen.
    pub fn hover_text(&mut self, text: &str) -> bool {
        let Some((_, rect)) = self.texts().into_iter().rev().find(|(t, _)| t == text) else {
            return false;
        };
        let _ = self.frame(vec![egui::Event::PointerMoved(rect.center())]);
        self.frames(2);
        true
    }

    pub fn click_text(&mut self, text: &str) -> bool {
        let Some((_, rect)) = self.texts().into_iter().rev().find(|(t, _)| t == text) else {
            return false;
        };
        self.click_at(rect.center());
        true
    }

    fn view_state(&self) -> ViewState {
        ViewState {
            main: self.app.views.main,
            tab_groups: self.app.views.groups.groups.len(),
            toolbars_locked: self.app.views.toolbars_locked,
            top: self.app.panes.top,
            bottom: self.app.panes.bottom,
            details: self.app.show_details,
            relationships: self.app.gantt.options.show_relationships,
            progress_line: self.app.gantt.options.progress_line.show,
            status_bar: self.app.views.show_status_bar,
            filter_active: self.app.grid.layout.filter.active(),
            collapsed: self.app.collapsed.clone(),
            line_numbers: self.app.grid.layout.table.line_numbers,
            hint_help: self.app.grid.layout.table.hint_help,
            spotlight: self.app.gantt.options.spotlight,
            reorganize_off: self.app.layout_opts.reorganize_off,
            critical_paths: self.app.critical_path.shown.is_some(),
            path_count: self.app.critical_path.count,
            path_group: self.app.critical_path.group,
            placing_text: self.app.gantt_ui.placing_text.is_some(),
        }
    }

    /// Panics with context when an error dialog is up.
    #[track_caller]
    pub fn assert_clean(&self, step: &str) {
        assert!(
            self.app.error.is_none(),
            "{step}: unexpected error dialog {:?}",
            self.app.error
        );
    }

    /// Keys of the rendered controls after the UI settles.
    pub fn keys(&mut self) -> BTreeSet<String> {
        self.frames(2);
        self.app.action_rects.keys().cloned().collect()
    }

    /// Close every dialog the last command opened: press its most cautious
    /// close button, or Escape when it has none, until no new close button
    /// is rendered. Returns the buttons pressed.
    pub(crate) fn close_dialogs(
        &mut self,
        baseline: &BTreeSet<String>,
        context: &str,
    ) -> Vec<String> {
        let mut pressed = vec![];
        for _ in 0..8 {
            let keys = self.keys();
            let mut closers: Vec<_> = keys
                .iter()
                .filter(|k| !baseline.contains(*k))
                .filter_map(|k| closer_rank(k).map(|rank| (rank, k.clone())))
                .collect();
            closers.sort();
            match closers.first() {
                Some((_, closer)) => {
                    let closer = closer.clone();
                    self.click(&closer);
                    pressed.push(closer);
                }
                None => break,
            }
        }
        let left: Vec<_> = self
            .keys()
            .into_iter()
            .filter(|k| !baseline.contains(k) && closer_rank(k).is_some())
            .collect();
        assert!(
            left.is_empty(),
            "{context}: a dialog did not close; still showing {left:?} after {pressed:?}"
        );
        // An open popup menu has no close button: Escape closes it.
        self.key(Key::Escape);
        pressed
    }

    /// The Activities view with the Gantt on top and Activity Details below,
    /// nothing open.
    pub fn home(&mut self) {
        self.key(Key::Escape);
        if self.app.views.groups.groups.len() > 1 {
            self.app.merge_tab_groups();
        }
        if self.app.views.main != views::MainView::Activities {
            self.click("directory_activities");
        }
        self.app.panes.top = panes::TopView::Gantt;
        self.app.panes.bottom = panes::BottomView::Details;
        self.app.show_details = true;
        self.frames(2);
    }

    /// Click `action`, close what it opened, and check that the view and the
    /// project are as before (toggles are clicked a second time). Returns
    /// the close buttons pressed (empty when no dialog opened).
    pub fn try_command(
        &mut self,
        action: &str,
        baseline: &BTreeSet<String>,
        context: &str,
    ) -> Vec<String> {
        let model = self.app.project.model.clone();
        let undo = self.app.history.counts().0;
        let before = self.view_state();
        if !self.has(action) {
            return vec![];
        }
        let rows_before = self.visible_activities().len();
        // A formatting survey needs each command once; the menu sweep lists
        // the direct items again for every submenu.
        if self.probe.enabled && !self.probe.tried.insert(action.to_string()) {
            return vec![];
        }
        self.click(action);
        let opened_dialog = self
            .keys()
            .iter()
            .any(|k| !baseline.contains(k) && closer_rank(k).is_some());
        if self.probe.enabled {
            self.explore(action, baseline);
        }
        let pressed = self.close_dialogs(baseline, &format!("{context} {action}"));
        assert!(
            self.app.error.is_none(),
            "{context} {action}: unexpected error dialog {:?}",
            self.app.error
        );
        let after = self.view_state();
        if after != before && pressed.is_empty() {
            // A toggle: its second click restores the state.
            self.frames(1);
            if self.has(action) {
                self.click(action);
                self.key(Key::Escape);
            }
        }
        if self.view_state() != before {
            // Collapse All and the like: expand the bands again.
            self.app.collapsed = before.collapsed.clone();
            // The Activity Critical Path commands live in a submenu that
            // closes after a click, so their second click cannot reach them:
            // put the paths back directly. They are a filter on the table,
            // which must not outlive the command that turned it on.
            if !before.critical_paths && self.app.critical_path.shown.is_some() {
                self.app.hide_critical_paths();
            }
            if self.app.critical_path.count != before.path_count
                || self.app.critical_path.group != before.path_group
            {
                self.app.critical_path.count = before.path_count;
                self.app.critical_path.group = before.path_group;
                self.app.drop_frame_cache();
            }
            self.home();
            if self.app.grid.layout.filter.active() != before.filter_active {
                self.app.grid.layout.filter.all_activities = true;
                self.app.drop_frame_cache();
            }
        }
        assert_eq!(
            self.visible_activities().len(),
            rows_before,
            "{context} {action}: the table lists a different set of activities after the command"
        );
        assert_eq!(
            self.app.history.counts().0,
            undo,
            "{context} {action}: opening and cancelling must not add an undo step"
        );
        assert!(
            self.app.project.model == model,
            "{context} {action}: opening and cancelling changed the project"
        );
        self.opened_dialog = opened_dialog;
        pressed
    }

    /// Opens `action`'s dialog again and presses Enter: the default button
    /// runs, which closes the dialog (a dialog may open a follow-up one; a
    /// step it records is undone, and what is left is closed).
    pub(crate) fn enter_runs_default(
        &mut self,
        action: &str,
        baseline: &BTreeSet<String>,
        context: &str,
        reopen: &dyn Fn(&mut Driver),
    ) {
        if !self.enter_check || !self.opened_dialog {
            return;
        }
        self.home();
        reopen(self);
        self.frames(2);
        // A command of the parent menu, listed again while a submenu was
        // open, is covered where its own menu is open.
        if !self.has(action) {
            self.reset_app();
            return;
        }
        self.click(action);
        self.frames(3);
        let closers = |d: &mut Driver| -> BTreeSet<String> {
            d.keys()
                .into_iter()
                .filter(|k| !baseline.contains(k) && closer_rank(k).is_some())
                .collect()
        };
        let opened = closers(self);
        if opened.is_empty() {
            self.key(Key::Escape);
            return;
        }
        let default = crate::dialog_keys::top_default_label(&self.ctx);
        let (live, layers) = crate::dialog_keys::top_live_buttons(&self.ctx);
        if std::env::var_os("OPENXER_QA_KEYS_LOG").is_some() {
            println!("ENTER {context} {action}: default {default:?}, closers {opened:?}");
        }
        self.key(Key::Enter);
        self.frames(3);
        let still: Vec<String> = closers(self).intersection(&opened).cloned().collect();
        let keeps = default
            .as_deref()
            .is_some_and(|d| ENTER_KEEPS_OPEN.contains(&d));
        let nameless = default.is_none() && !live.is_empty() && !ENTER_VIEWERS.contains(&action);
        let stuck =
            !still.is_empty() && !keeps && default.is_some() && !ENTER_VIEWERS.contains(&action);
        if stuck || nameless {
            let message =
                format!(
                "{context} {action}: Enter (default {default:?}, buttons {live:?}, {layers} dialogs) left {still:?} open"
            );
            if std::env::var_os("OPENXER_QA_KEYS_LOG").is_some() {
                println!("ENTERFAIL {message}");
            } else {
                panic!("{message}");
            }
        }
        // The default button may have changed anything: start over.
        self.reset_app();
    }

    /// Clicks a menu bar title; the first click on a freshly started
    /// application can be lost, so it is tried twice.
    pub fn open_menu(&mut self, title: &str) {
        self.click(title);
        if !egui::Popup::is_any_open(&self.ctx) {
            self.click(title);
        }
    }

    /// A fresh application on the project the driver started with, set up
    /// as the sweep started.
    pub(crate) fn reset_app(&mut self) {
        let project = self.origin.clone().expect("the driver kept its project");
        let mut fresh = Driver::build("reset", project, self.screen, Default::default());
        std::mem::swap(&mut self.app, &mut fresh.app);
        std::mem::swap(&mut self.ctx, &mut fresh.ctx);
        std::mem::swap(&mut self.dir, &mut fresh.dir);
        // The clock goes on: a new context starts at the old time.
        self.time += fresh.time;
        self.frames(6);
        if let Some(setup) = self.setup {
            setup(self);
        }
        self.home();
        self.frames(6);
    }

    /// Open every top-level menu and submenu, run each command that opens a
    /// dialog and cancel it; toggles are toggled back.
    pub fn sweep_menus(&mut self) -> usize {
        self.home();
        let baseline = self.keys();
        let mut tried = 0;
        for (menu, subs) in MENUS {
            tried += self.sweep_menu(menu, subs, &baseline);
        }
        self.home();
        tried
    }

    /// One top-level menu: its direct items, then each submenu's items.
    pub fn sweep_menu(&mut self, menu: &str, subs: &[&str], baseline: &BTreeSet<String>) -> usize {
        let title = format!("menu_bar_{menu}");
        let mut tried = 0;
        let mut groups: Vec<Option<&str>> = vec![None];
        groups.extend(subs.iter().map(|s| Some(*s)));
        for sub in groups {
            self.home();
            self.open_menu(&title);
            if let Some(label) = sub {
                if !self.hover_text(label) {
                    let shown: Vec<String> = self.texts().into_iter().map(|(t, _)| t).collect();
                    panic!(
                        "{menu} menu: submenu {label:?} is not drawn; open {}; text {:?}",
                        self.has(&format!("menu_bar_{menu}")),
                        &shown[shown.len().saturating_sub(40)..]
                    );
                }
            }
            let items: Vec<String> = self
                .keys()
                .into_iter()
                .filter(|k| !baseline.contains(k) && !never_click(k))
                .collect();
            self.key(Key::Escape);
            self.key(Key::Escape);
            for item in items {
                let key = format!("{menu}|{}|{item}", sub.unwrap_or(""));
                if !in_slice(&key, self.slice) {
                    continue;
                }
                self.home();
                self.open_menu(&title);
                if let Some(label) = sub {
                    self.hover_text(label);
                }
                if !self.has(&item) {
                    // Items of another (sibling) submenu that closed.
                    COMMANDS.with(|c| c.borrow_mut().push(format!("gone {key}")));
                    continue;
                }
                COMMANDS.with(|c| c.borrow_mut().push(format!("tried {key}")));
                let context = format!("{menu} > {}", sub.unwrap_or(""));
                self.try_command(&item, baseline, &context);
                let title = title.clone();
                self.enter_runs_default(&item, baseline, &context, &|d: &mut Driver| {
                    d.open_menu(&title);
                    if let Some(label) = sub {
                        d.hover_text(label);
                    }
                });
                tried += 1;
            }
        }
        self.home();
        tried
    }

    /// Every toolbar and dock button in the Activities view.
    pub fn sweep_toolbars(&mut self) -> usize {
        self.home();
        let baseline = self.keys();
        let buttons: Vec<String> = baseline
            .iter()
            .filter(|k| {
                let r = self.rect(k);
                let toolbar = r.top() < menu::MENU_ROW + 2. * menu::TOOLBAR_ROW;
                let dock = r.right() < 34. || r.left() > self.screen.x - 34.;
                (toolbar || dock) && !never_click(k)
            })
            .filter(|k| in_slice(&format!("toolbar|{k}"), self.slice))
            .cloned()
            .collect();
        for button in &buttons {
            self.home();
            self.try_command(button, &baseline, "toolbar");
            self.enter_runs_default(button, &baseline, "toolbar", &|_| {});
        }
        self.home();
        buttons.len() + self.sweep_chevrons()
    }

    /// Every bar's chevron menu: it opens, Customize... opens the dialog on
    /// that bar and closes again, the first button goes off from Add or
    /// Remove Buttons while the list stays open, Reset Toolbar brings it
    /// back, and Escape closes the menu. Returns the menu commands tried.
    pub fn sweep_chevrons(&mut self) -> usize {
        self.home();
        let baseline = self.keys();
        let chevrons: Vec<String> = baseline
            .iter()
            .filter(|k| k.starts_with("bar_chevron_"))
            .cloned()
            .collect();
        assert!(chevrons.len() > 5, "the bars have chevrons: {chevrons:?}");
        let mut tried = 0;
        for chevron in &chevrons {
            if !in_slice(&format!("toolbar|{chevron}"), self.slice) {
                continue;
            }
            let bar = chevron.trim_start_matches("bar_chevron_").to_string();
            let context = format!("{bar} chevron");
            self.home();
            self.click(chevron);
            assert!(self.has("chevron_customize"), "{context}: no menu");
            self.try_command("chevron_customize", &baseline, &context);
            assert!(self.app.views.customize.is_none(), "{context}: Customize");
            tried += 1;
            self.home();
            let Some(index) = crate::toolbars::bar_index(&bar) else {
                continue;
            };
            let first = crate::toolbars::TOOLBAR_BUTTONS[index].1[0].0;
            self.click(chevron);
            assert!(
                self.hover_text("Add or Remove Buttons"),
                "{context}: no Add or Remove Buttons"
            );
            let item = format!("chevron_button_{first}");
            self.click(&item);
            assert!(!self.app.button_visible(first), "{context}: {first} stays");
            assert!(self.has(&item), "{context}: the list closed");
            self.key(Key::Escape);
            self.frames(2);
            assert!(!self.has("chevron_customize"), "{context}: Escape");
            self.click(chevron);
            self.click("chevron_reset");
            assert!(
                self.app.views.hidden_buttons.is_empty(),
                "{context}: Reset Toolbar"
            );
            tried += 2;
            self.assert_clean(&context);
        }
        self.home();
        tried
    }

    /// Every main view, every bottom pane, the network, and every visible
    /// Activity Details tab.
    pub fn sweep_views(&mut self) {
        self.home();
        for view in [
            "directory_wbs",
            "directory_projects",
            "directory_resources",
            "dock_reports",
            "directory_activities",
        ] {
            self.click(view);
            self.frames(2);
            self.assert_clean(view);
            let chart = match view {
                "directory_wbs" => Some("wbs"),
                "directory_projects" => Some("projects"),
                _ => None,
            };
            if let Some(id) = chart {
                self.sweep_timeline(&format!("{id}_hscroll"), &format!("{id}_chart"));
            }
        }
        self.sweep_timeline("hscroll_gantt", "timescale_zoom");
        self.click("menu_bar_project");
        self.click("menu_resource_assignments");
        assert_eq!(self.app.views.main, views::MainView::Assignments);
        self.frames(2);
        self.assert_clean("resource assignments view");
        self.sweep_timeline("assignments_hscroll", "assignments_chart");
        self.click("directory_activities");
        // Activity Network on top, then back to the Gantt.
        self.click("tool_network");
        assert_eq!(self.app.panes.top, panes::TopView::Network);
        self.frames(2);
        self.click("tool_gantt");
        assert_eq!(self.app.panes.top, panes::TopView::Gantt);
        // Every bottom pane, each from the Bottom Layout toolbar.
        for pane in [
            "tool_activity_usage_sheet",
            "tool_activity_usage_profile",
            "tool_resource_usage_sheet",
            "tool_resource_usage_profile",
            "trace",
        ] {
            self.click(pane);
            self.frames(2);
            assert!(self.app.show_details, "{pane} shows a bottom pane");
            self.assert_clean(pane);
            for name in ["usage_sheet_timescale", "usage_profile_timescale"] {
                if self.has(&format!("{name}_hscroll")) {
                    self.sweep_timeline(&format!("{name}_hscroll"), &format!("{name}_body"));
                }
            }
        }
        self.sweep_usage_lists();
        self.home();
        // Every Activity Details tab that is on the strip.
        let tabs: Vec<String> = self
            .keys()
            .into_iter()
            .filter(|k| k.starts_with("detail_tab_"))
            .collect();
        assert!(tabs.len() >= 10, "details tab strip: {tabs:?}");
        for tab in tabs {
            self.click(&tab);
            self.frames(1);
            self.assert_clean(&tab);
        }
        self.click("detail_tab_general");
        self.sweep_view_customizers();
    }

    /// The resource list of both resource usage panes: every Filter By,
    /// Group and Sort By and Select View choice from its Display bar, the
    /// Customize... dialogs (Cancel and Escape), a header sort, Expand All
    /// and Collapse All, and the Display Activities for selected boxes.
    pub fn sweep_usage_lists(&mut self) {
        let before = self.app.panes.usage.clone();
        for (pane, tool) in [
            ("sheet", "tool_resource_usage_sheet"),
            ("profile", "tool_resource_usage_profile"),
        ] {
            self.click(tool);
            self.frames(2);
            let bar = format!("usage_list_bar_{pane}");
            assert!(self.has(&bar), "{pane}: the list's Display bar");
            let pick = |d: &mut Self, path: &[&str]| {
                d.click(&bar);
                d.frames(1);
                let (last, subs) = path.split_last().unwrap();
                for sub in subs {
                    let at = d.rect(&format!("{bar}_{sub}")).center();
                    let _ = d.frame(vec![egui::Event::PointerMoved(at)]);
                    d.frames(2);
                }
                d.click(&format!("{bar}_{last}"));
                d.frames(2);
                d.key(Key::Escape);
                d.assert_clean(&format!("{pane} list {path:?}"));
            };
            for path in [
                &["filter_by", "filter_all"][..],
                &["group_by", "group_list"],
                &["group_by", "group_role"],
                &["group_by", "group_type"],
                &["group_by", "group_hierarchy"],
                &["collapse_all"],
                &["expand_all"],
                &["select_view", "view_role"],
                &["filter_by", "filter_project"],
                &["select_view", "view_resource"],
                &["filter_by", "filter_project"],
            ] {
                pick(self, path);
            }
            for (path, cancel, open) in [
                (
                    ["filter_by", "filter_custom"],
                    "usage_list_filter_cancel",
                    0,
                ),
                (["group_by", "group_custom"], "usage_list_group_cancel", 1),
                (["columns", "columns_customize"], "usage_fields_cancel", 2),
            ] {
                let is_open = |d: &Self| match open {
                    0 => d.app.panes.usage_list_filter.is_some(),
                    1 => d.app.panes.usage_list_group.is_some(),
                    _ => d.app.panes.usage_fields.is_some(),
                };
                for escape in [false, true] {
                    self.click(&bar);
                    self.frames(1);
                    let at = self.rect(&format!("{bar}_{}", path[0])).center();
                    let _ = self.frame(vec![egui::Event::PointerMoved(at)]);
                    self.frames(2);
                    self.click(&format!("{bar}_{}", path[1]));
                    self.frames(2);
                    assert!(is_open(self), "{pane}: {path:?} opens its dialog");
                    if escape {
                        self.key(Key::Escape);
                    } else {
                        self.click(cancel);
                    }
                    self.frames(2);
                    assert!(!is_open(self), "{pane}: {path:?} closes");
                    self.assert_clean(&format!("{pane} {path:?}"));
                }
            }
            let head = format!("usage_list_head_{pane}_0");
            if self.has(&head) {
                self.click(&head);
                self.click(&head);
                pick(self, &["group_by", "sort_clear"]);
            }
            for check in ["resource", "time_period"] {
                let name = format!("usage_list_{pane}_{check}");
                self.click(&name);
                self.click(&name);
            }
            self.assert_clean(&format!("{pane} list"));
        }
        // Nothing here is project data; the list choices go back as they were.
        let u = &mut self.app.panes.usage;
        u.profile_list = before.profile_list;
        u.sheet_list = before.sheet_list;
        u.resource_filter = before.resource_filter;
        u.time_period = before.time_period;
    }

    /// The view commands that change no project data: Customize Columns on
    /// every record view and on the Resources view's rate grid (opened,
    /// then cancelled), the Resource Assignments Layout > Open... and Save
    /// As... windows (closed with Escape and Cancel), and View > Show on Top
    /// > Resource Usage Spreadsheet.
    pub fn sweep_view_customizers(&mut self) {
        for (view, id) in [
            (views::MainView::Expenses, "expenses"),
            (views::MainView::Documents, "documents"),
            (views::MainView::Risks, "risks"),
            (views::MainView::Issues, "issues"),
            (views::MainView::Thresholds, "thresholds"),
        ] {
            self.app.show_view(view);
            self.frames(3);
            let at = match self.app.action_rects.get(&format!("{id}_row_0")) {
                Some(r) => Pos2::new(r[0] + 90., (r[1] + r[3]) / 2.),
                None => {
                    let r = self.rect(&format!("{id}_layout"));
                    Pos2::new(r.center().x, r.bottom() + 60.)
                }
            };
            self.press_at(at, egui::PointerButton::Secondary, Default::default());
            self.click(&format!("{id}_menu_customize"));
            assert!(
                self.app.detail.column_dialog.is_some(),
                "{id}: Customize Columns opens"
            );
            self.click("detail_columns_cancel");
            assert!(
                self.app.detail.column_dialog.is_none(),
                "{id}: Cancel closes"
            );
            self.assert_clean(&format!("{id} customize columns"));
        }
        // The rate grid of the first resource with rates.
        self.click("directory_resources");
        let first = crate::views::resources::resources(&self.app.project)
            .into_iter()
            .map(|r| r.id)
            .find(|&id| {
                !crate::views::resource_edit::rate_rows(&self.app.project.document, id).is_empty()
            });
        if let Some(id) = first {
            let r = &mut self.app.views.resources;
            r.assigned_only = false;
            r.selected = Some(id);
            r.show_bottom = true;
            r.tab = 3;
            self.frames(3);
            if self.has("resource_rates_row_0") {
                let at = self.rect("resource_rates_row_0").center();
                self.press_at(at, egui::PointerButton::Secondary, Default::default());
                self.click("resource_rates_menu_customize");
                assert!(self.app.detail.column_dialog.is_some(), "rate columns open");
                self.key(Key::Escape);
                self.frames(2);
                assert!(self.app.detail.column_dialog.is_none(), "Escape closes");
                self.assert_clean("resource rate columns");
            }
        }
        // Resource Assignments: Layout > Open... and Save As...
        self.app.show_view(views::MainView::Assignments);
        self.frames(3);
        let header = self.rect("assignments_col_0");
        let at = Pos2::new(header.center().x, header.bottom() + 27.);
        for (item, close) in [("open", "escape"), ("save_as", "save_as_cancel")] {
            self.press_at(at, egui::PointerButton::Secondary, Default::default());
            let hover = self.rect("assignments_menu_layout").center();
            let _ = self.frame(vec![egui::Event::PointerMoved(hover)]);
            self.frames(3);
            self.click(&format!("assignments_menu_layout_{item}"));
            let open = |d: &Self| {
                d.app.grid.dialogs.open_layout.is_some() || d.app.grid.dialogs.save_as.is_some()
            };
            assert!(open(self), "assignments Layout > {item} opens");
            if close == "escape" {
                self.key(Key::Escape);
                self.frames(2);
            } else {
                self.click(close);
            }
            assert!(!open(self), "assignments Layout > {item} closes");
            self.assert_clean(&format!("assignments layout {item}"));
        }
        // Show on Top > Resource Usage Spreadsheet, then back to the Gantt.
        self.click("directory_activities");
        self.click("menu_bar_view");
        assert!(self.hover_text("Show on Top"), "Show on Top is drawn");
        self.click("menu_top_resource_usage");
        self.frames(2);
        assert_eq!(self.app.panes.top, panes::TopView::ResourceUsageSheet);
        self.assert_clean("resource usage on top");
        self.click("tool_gantt");
        assert_eq!(self.app.panes.top, panes::TopView::Gantt);
    }

    /// The Projects view's Funding tab: Add then Delete (two undo steps),
    /// or, with no funding sources defined, Add opening Enterprise >
    /// Funding Sources (closed with Escape).
    pub fn sweep_funding(&mut self) {
        self.click("directory_projects");
        let p = &self.app.project;
        self.app.views.projects.selected = Some((p.source_name.clone(), p.model.project.proj_id));
        self.app.views.projects.show_details = true;
        self.app.views.projects.tab = crate::views::projects::TABS
            .iter()
            .position(|t| *t == "Funding")
            .unwrap();
        self.frames(3);
        let before = openxer_xer::edit::funding::list(&self.app.project).len();
        self.click("project_funding_buttons_0");
        self.frames(2);
        if self.app.enterprise_open() {
            self.key(Key::Escape);
            self.frames(2);
            assert!(!self.app.enterprise_open(), "Escape closes Funding Sources");
        } else if openxer_xer::edit::funding::list(&self.app.project).len() == before + 1 {
            let last = before;
            self.app.views.projects.funding_row = Some(last);
            self.frames(2);
            self.click("project_funding_buttons_1");
            self.frames(2);
            assert_eq!(
                openxer_xer::edit::funding::list(&self.app.project).len(),
                before,
                "Delete removes the added funding"
            );
        }
        self.assert_clean("funding add and delete");
        self.click("directory_activities");
    }

    /// Resources > Del. / Merge opens and Escape closes
    /// it unchanged; the Stacked Histogram by resource and by role on both
    /// bottom profiles; one edit on the WBS Earned Value tab.
    pub fn sweep_batch3(&mut self) {
        self.click("directory_resources");
        self.app.views.resources.assigned_only = false;
        self.frames(2);
        let first = crate::views::resources::resources(&self.app.project)
            .first()
            .map(|r| r.id);
        if let Some(id) = first {
            let before = self.app.project.document.text();
            self.app.views.resources.selected = Some(id);
            self.app.open_resource_merge();
            self.frames(2);
            assert!(
                self.app.views.resources.merge.is_some(),
                "Del. / Merge opens"
            );
            self.key(Key::Escape);
            self.frames(2);
            assert!(self.app.views.resources.merge.is_none(), "Escape closes it");
            assert_eq!(
                self.app.project.document.text(),
                before,
                "Escape leaves the project unchanged"
            );
        }
        self.assert_clean("resources del / merge");
        self.click("directory_activities");
        let (bottom, shown) = (self.app.panes.bottom, self.app.show_details);
        for view in [
            crate::panes::BottomView::ActivityUsageProfile,
            crate::panes::BottomView::ResourceUsageProfile,
        ] {
            self.app.set_bottom(Some(view));
            for by in [
                crate::panes::stacked::StackBy::Resource,
                crate::panes::stacked::StackBy::Role,
            ] {
                self.app.panes.usage.stack_activity = true;
                self.app.panes.usage.stack_resource = true;
                self.app.panes.usage.stack_by = by;
                self.frames(2);
            }
            self.app.panes.usage.stack_activity = false;
            self.app.panes.usage.stack_resource = false;
            self.frames(1);
        }
        self.app.set_bottom(Some(bottom));
        if !shown {
            self.app.set_bottom(None);
        }
        self.assert_clean("stacked histogram");
        self.click("directory_wbs");
        self.app.views.wbs.show_details = true;
        self.app.views.wbs.tab = crate::views::wbs::TABS
            .iter()
            .position(|t| *t == "Earned Value")
            .unwrap();
        self.frames(3);
        if self.has("wbs_ev_50_50") {
            self.click("wbs_ev_50_50");
            self.frames(2);
        }
        self.assert_clean("wbs earned value");
        self.click("directory_activities");
    }

    /// A chart's timeline: its scroll bar arrows (`bar`), Ctrl+wheel and
    /// Shift+wheel over `body`, and View > Zoom In and Out. Each leaves the
    /// app without an error or a stray window.
    pub fn sweep_timeline(&mut self, bar: &str, body: &str) {
        for arrow in ["_right", "_left"] {
            let name = format!("{bar}{arrow}");
            if self.has(&name) {
                self.click(&name);
            }
        }
        if self.has(body) {
            let at = self.rect(body).center();
            for (delta, modifiers) in [
                (Vec2::new(0., 120.), egui::Modifiers::CTRL),
                (Vec2::new(0., -120.), egui::Modifiers::CTRL),
                (Vec2::new(0., -120.), egui::Modifiers::SHIFT),
            ] {
                let _ = self.frame_with(
                    vec![
                        egui::Event::PointerMoved(at),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta,
                            modifiers,
                        },
                    ],
                    modifiers,
                );
                self.frames(1);
            }
            let _ = self.frame(vec![egui::Event::PointerGone]);
        }
        if self.app.timeline_on_screen() {
            self.app.zoom_step(true);
            self.frames(1);
            self.app.zoom_step(false);
            self.frames(1);
        }
        self.assert_clean(bar);
    }

    /// Activities in table order, as drawn.
    pub fn visible_activities(&self) -> Vec<usize> {
        self.app
            .rows()
            .into_iter()
            .filter_map(|r| match r {
                Row::Activity(i) => Some(i),
                Row::Group(_) => None,
            })
            .collect()
    }

    /// The center of activity `i`'s cell under column `id`, when drawn.
    pub fn cell(&self, i: usize, id: &str) -> Option<Pos2> {
        let task_id = self.app.project.model.activities[i].task_id;
        let row = self
            .app
            .bar_geometry
            .iter()
            .find(|b| b.task_id == task_id)?
            .row;
        let header = format!("grid_header_{id}");
        if !self.has(&header) {
            return None;
        }
        let header = self.rect(&header);
        // Fully inside the table: below its header, above the bottom pane
        // (whose tab strip is its first row) or the status bar.
        let table_bottom = self
            .app
            .action_rects
            .iter()
            .filter(|(k, _)| k.starts_with("detail_tab_"))
            .map(|(_, r)| r[1] - 8.)
            .fold(self.screen.y - 40., f32::min);
        (row.top() >= header.bottom() && row.bottom() < table_bottom)
            .then(|| Pos2::new(header.center().x, row.center().y))
    }

    pub fn select(&mut self, i: usize) {
        self.frames(1);
        if self.cell(i, "activity_id").is_none() {
            // Off screen: scroll it into view the way Go To does.
            self.app.selected = Some(i);
            self.app.scroll_to_selected = true;
            self.frames(3);
        }
        let Some(pos) = self.cell(i, "activity_id") else {
            let task_id = self.app.project.model.activities[i].task_id;
            panic!(
                "activity row is drawn: {} row {:?}, header {:?}, {} bars, in rows {}, main {:?}, details {}",
                self.app.project.model.activities[i].task_code,
                self.app.bar_geometry.iter().find(|b| b.task_id == task_id).map(|b| b.row),
                self.app.action_rects.get("grid_header_activity_id"),
                self.app.bar_geometry.len(),
                self.visible_activities().contains(&i),
                self.app.views.main,
                self.app.show_details
            )
        };
        self.click_at(pos);
        assert_eq!(
            self.app.selected,
            Some(i),
            "clicked {pos:?} for {}",
            self.app.project.model.activities[i].task_code
        );
        // A click on the focused cell of the current row opens its editor;
        // selecting is all the sweep wants.
        if self.app.edit_cell.is_some() {
            self.key(Key::Escape);
        }
    }

    /// Run F9 through the Schedule dialog.
    pub fn f9(&mut self) -> bool {
        let runs = self.app.schedule_runs;
        self.key(Key::F9);
        assert!(self.app.sched.schedule_open, "F9 opens the Schedule dialog");
        self.click("schedule_run");
        if let Some(e) = self.app.error.take() {
            panic!("F9 failed: {e}");
        }
        assert_eq!(self.app.schedule_runs, runs + 1, "F9 ran once");
        assert!(!self.app.sched.schedule_open);
        true
    }

    pub fn level(&mut self) {
        self.home();
        let baseline = self.keys();
        self.click("tool_level");
        self.click("level_run");
        self.frames(2);
        if let Some(e) = self.app.error.take() {
            self.notes.push(format!("Level refused: {e}"));
        }
        // Level closes the dialog; a refusal leaves it open with its message.
        self.close_dialogs(&baseline, "level resources");
        assert!(
            self.app.sched.progress.level.is_none(),
            "Level Resources closed"
        );
        self.app.show_log = false;
        self.frames(1);
    }

    pub fn check_schedule(&mut self) {
        self.home();
        let baseline = self.keys();
        self.click("menu_bar_tools");
        self.click("menu_check_schedule");
        self.click("check_run");
        self.frames(2);
        self.assert_clean("check schedule");
        assert!(!self.app.sched.check_open, "Check Schedule closes on Run");
        assert!(
            self.app.sched.check_results.is_some(),
            "Check Schedule shows its results"
        );
        assert!(Path::new(&self.app.sched.check_report_path).exists());
        // The results window closes from its title bar button.
        let window = self
            .ctx
            .memory(|m| m.area_rect(egui::Id::new("check_results")))
            .expect("results window is drawn");
        self.click_at(Pos2::new(window.right() - 12., window.top() + 12.));
        assert!(
            self.app.sched.check_results.is_none(),
            "Check Schedule Results closes"
        );
        self.close_dialogs(&baseline, "check schedule");
    }

    pub fn undo_all(&mut self) -> usize {
        let mut n = 0;
        while self.app.history.counts().0 > 0 {
            self.click("undo");
            n += 1;
            assert!(n <= 70, "undo did not empty the history");
        }
        n
    }

    pub fn redo_all(&mut self) -> usize {
        let mut n = 0;
        while self.app.history.counts().1 > 0 {
            self.click("redo");
            n += 1;
            assert!(n <= 70, "redo did not empty the history");
        }
        n
    }

    /// File > Import of one XER file through every wizard page (Create New).
    pub fn import(&mut self, path: &Path) {
        self.home();
        self.click("menu_bar_file");
        self.click("file_import");
        assert!(self.app.interchange.xer.is_some(), "Import wizard opens");
        self.click("import_format_xer");
        self.click("xer_import_next");
        // Type the file name into the File Name box (focused by a click on it).
        self.click("xer_import_file");
        self.type_text(&path.display().to_string());
        assert_eq!(
            self.app.interchange.xer.as_ref().unwrap().path_text,
            path.display().to_string()
        );
        self.click("xer_import_next");
        let w = self.app.interchange.xer.as_ref().unwrap();
        assert!(w.error.is_none(), "{:?}", w.error);
        assert_eq!(w.step, 2);
        self.click("xer_import_next");
        let w = self.app.interchange.xer.as_ref().unwrap();
        assert!(w.error.is_none(), "{:?}", w.error);
        assert_eq!(w.step, 4, "Create New skips the update options page");
        self.click("xer_import_finish");
        self.assert_clean("import");
        assert!(self.app.interchange.xer.is_none());
        self.click("interchange_message_ok");
        assert!(self.app.interchange.message.is_none());
        self.frames(2);
    }

    /// File > Export through the wizard into `path`.
    pub fn export(&mut self, format: interchange::ExportFormat, path: &Path) {
        self.home();
        self.click("menu_bar_file");
        self.click("file_export");
        let action = match format {
            interchange::ExportFormat::Xer => "export_format_xer",
            interchange::ExportFormat::Xml => "export_format_xml",
            interchange::ExportFormat::Modern => "export_format_modern",
            interchange::ExportFormat::Tables => "export_format_tables",
        };
        self.click(action);
        for step in 1..=3 {
            self.click("export_next");
            let w = self.app.interchange.export.as_ref().unwrap();
            assert!(w.error.is_none(), "export step {step}: {:?}", w.error);
            assert_eq!(w.step, step);
        }
        self.app.interchange.export.as_mut().unwrap().path = path.display().to_string();
        let replacing = path.exists();
        self.click("export_finish");
        if replacing {
            // A file that is there already is replaced on a second Finish.
            let w = self.app.interchange.export.as_ref().expect("asks first");
            assert!(
                w.error
                    .as_deref()
                    .is_some_and(|e| e.contains("already exists")),
                "{:?}",
                w.error
            );
            self.click("export_finish");
        }
        assert!(
            self.app.interchange.export.is_none(),
            "export finished: {:?}",
            self.app
                .interchange
                .export
                .as_ref()
                .map(|w| w.error.clone())
        );
        self.click("interchange_message_ok");
        assert!(path.exists(), "{} written", path.display());
        self.assert_clean("export");
    }

    /// The spreadsheet import plan of `path` against the open project,
    /// through the wizard's first two pages, then Cancel.
    pub fn spreadsheet_plan(
        &mut self,
        path: &Path,
    ) -> (openxer_interchange::xlsx_import::PlanCounts, Vec<String>) {
        self.app.open_sheet_import(path.display().to_string());
        self.click("sheet_import_next");
        let w = self.app.interchange.sheet.as_ref().unwrap();
        assert!(w.error.is_none(), "{}: {:?}", path.display(), w.error);
        self.click("sheet_import_next");
        let plan = self
            .app
            .interchange
            .sheet
            .as_ref()
            .and_then(|w| w.plan.as_ref())
            .expect("the preview page has a plan");
        let counts = plan.counts();
        let samples = plan
            .rows
            .iter()
            .flat_map(|r| {
                r.changes
                    .iter()
                    .map(move |c| {
                        format!("{} {}: {:?} -> {:?}", r.activity_id, c.column, c.old, c.new)
                    })
                    .chain(
                        r.messages
                            .iter()
                            .map(move |m| format!("{}: {m}", r.activity_id)),
                    )
            })
            .take(6)
            .collect();
        self.click("sheet_import_cancel");
        assert!(self.app.interchange.sheet.is_none());
        (counts, samples)
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The model as a saved XER carries it, by the rule of
/// `xer/tests/corpus_roundtrip.rs`: completed activities keep internal
/// remaining dates that the XER leaves blank, and the engine's per-assignment
/// late dates are scheduling output the importer does not read.
pub(crate) fn as_saved(model: &Model) -> Model {
    let mut m = model.clone();
    for a in &mut m.activities {
        if a.is_completed() {
            a.rem_restart = a.restart;
            a.rem_reend = a.reend;
        }
        for x in &mut a.assignments {
            x.rem_late_start = None;
            x.rem_late_finish = None;
        }
    }
    m
}

/// The first differences between two models, for failure messages.
pub(crate) fn model_diff(a: &Model, b: &Model) -> String {
    let mut out = vec![];
    if a.activities.len() != b.activities.len() || a.links.len() != b.links.len() {
        out.push(format!(
            "sizes {}/{} activities, {}/{} links",
            a.activities.len(),
            b.activities.len(),
            a.links.len(),
            b.links.len()
        ));
    }
    for (x, y) in a.activities.iter().zip(&b.activities) {
        if x != y {
            let (p, q) = (format!("{x:#?}"), format!("{y:#?}"));
            let (p, q): (Vec<_>, Vec<_>) = (p.lines().collect(), q.lines().collect());
            let first = p.iter().zip(&q).position(|(l, r)| l != r).unwrap_or(0);
            out.push(format!(
                "activity {} (task_id {}): {:?} vs {:?}",
                x.task_code,
                x.task_id,
                &p[first..(first + 3).min(p.len())],
                &q[first..(first + 3).min(q.len())]
            ));
        }
        if out.len() > 4 {
            break;
        }
    }
    for (n, (x, y)) in a.links.iter().zip(&b.links).enumerate() {
        if x != y && out.len() < 8 {
            out.push(format!("link {n}: {x:?} vs {y:?}"));
        }
    }
    if a.project != b.project {
        out.push(format!("project: {:?} vs {:?}", a.project, b.project));
    }
    if a.options != b.options {
        out.push(format!("options: {:?} vs {:?}", a.options, b.options));
    }
    if a.calendars != b.calendars {
        out.push("calendars differ".into());
    }
    if (a.origin_serial, a.project_calendar) != (b.origin_serial, b.project_calendar) {
        out.push("origin or project calendar differ".into());
    }
    out.join("\n")
}

/// Dates, floats and flags that F9 computes, per Activity ID.
type Results = BTreeMap<String, [Option<i64>; 10]>;

fn results(model: &Model) -> Results {
    model
        .activities
        .iter()
        .map(|a| {
            (
                a.task_code.clone(),
                [
                    a.early_start,
                    a.early_finish,
                    a.late_start,
                    a.late_finish,
                    a.rem_late_start,
                    a.rem_late_finish,
                    a.total_float,
                    a.free_float,
                    Some(a.driving_path as i64),
                    Some(a.task_type as i64),
                ],
            )
        })
        .collect()
}

fn assert_same_results(a: &Model, b: &Model, context: &str) {
    let (a, b) = (results(a), results(b));
    assert_eq!(a.len(), b.len(), "{context}: activity count");
    let differ: Vec<_> = a
        .iter()
        .filter(|(k, v)| b.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), *v, b.get(k).copied()))
        .take(5)
        .collect();
    assert!(differ.is_empty(), "{context}: results differ {differ:?}");
}

/// A link that the validation accepts: predecessor before successor in the
/// table, no existing link either way, both bars drawn with their ends inside
/// the chart.
fn link_candidates(d: &Driver) -> Vec<(i64, i64)> {
    // Rows scrolled up behind the table header are not on screen: a press at
    // their bars lands on the header and timescale instead.
    let header_bottom = d
        .app
        .action_rects
        .get("grid_header_activity_id")
        .map_or(120., |r| r[3].max(120.));
    let bars: Vec<_> = d
        .app
        .bar_geometry
        .iter()
        .filter(|b| {
            b.start.x > d.app.gantt_left + 4.
                && b.finish.x < SCREEN.x - 40.
                && b.row.top() > header_bottom
                && b.row.bottom() < 560.
                && b.finish.x - b.start.x > 2.
        })
        .map(|b| b.task_id)
        .collect();
    let m = &d.app.project.model;
    let index = |id| m.activities.iter().position(|a| a.task_id == id).unwrap();
    let mut out = vec![];
    for (n, &p) in bars.iter().enumerate() {
        for &s in bars.iter().skip(n + 1) {
            let (pi, si) = (index(p), index(s));
            let linked = m
                .links
                .iter()
                .any(|l| (l.pred == pi && l.succ == si) || (l.pred == si && l.succ == pi));
            if !linked {
                out.push((p, s));
            }
        }
    }
    out
}

/// Lap times of a sweep test, written to the temporary folder
/// (`openxer-qa-sweep\<process>-<test>.log`, never inside the repository),
/// because a passing test's printed output is not shown. Lines name a
/// project by its tag (`starter`, or a corpus file's position in the sorted
/// list), never by its file name.
pub(crate) struct Laps {
    path: Option<PathBuf>,
}

impl Laps {
    pub(crate) fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join("openxer-qa-sweep");
        let path = dir.join(format!("{}-{test}.log", std::process::id()));
        let ok = std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&path, "").is_ok();
        Self {
            path: ok.then_some(path),
        }
    }

    pub(crate) fn line(&self, text: &str) {
        use std::io::Write;
        if let Some(path) = &self.path {
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(path) {
                let _ = writeln!(f, "{text}");
            }
        }
    }
}

thread_local! {
    /// The menu commands the sweep tried or found gone on reopening their
    /// menu, for the lap log (`tried view|Layout|menu_layout_open`).
    static COMMANDS: std::cell::RefCell<Vec<String>> = Default::default();
}

/// Which part of the scenario one sweep test runs. Both parts start with
/// the views sweep on a freshly imported project.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    /// One slice of the menu and toolbar commands (see [`in_slice`]), then
    /// the check that browsing changed nothing. Every command is in exactly
    /// one of the `of` slices, so the slices together try every command.
    Browse { slice: usize, of: usize },
    /// The check that browsing the views changed nothing, then F9,
    /// leveling, Check Schedule, the edits, undo and redo, save and reopen,
    /// and the exports.
    Rest,
}

/// The menu and toolbar commands of a project with the full sweep are
/// split into this many slices, each run by its own test on a fresh import,
/// so the slices run side by side instead of one after another.
const MENU_SLICES: usize = 6;

/// Whether the command `key` (menu, submenu and action, or a toolbar
/// button) belongs to `slice`. The slice comes from a hash of the key, not
/// from the command's position, so it does not depend on what an earlier
/// command left on screen; each key is in exactly one slice.
fn in_slice(key: &str, slice: Option<(usize, usize)>) -> bool {
    let Some((slice, of)) = slice else {
        return true;
    };
    // FNV-1a, so the split is the same on every run and every machine.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash % of as u64) as usize == slice
}

/// The scenario, or the part of it `part` names, on the project the driver
/// has open.
fn scenario(d: &mut Driver, label: &str, part: Part, laps: &Laps, tag: &str) {
    let started = Instant::now();
    let lap = |name: &str, d: &Driver| {
        let secs = started.elapsed().as_secs_f64();
        println!("  {label} {name}: {secs:.1}s");
        laps.line(&format!("{tag} {name}: {secs:.1}s"));
        d.assert_clean(name);
    };
    let imported_model = d.app.project.model.clone();
    let imported_details = d.app.project.details.clone();
    assert_eq!(d.app.history.counts(), (0, 0));

    d.sweep_views();
    lap("views", d);
    if let Part::Browse { slice, of } = part {
        d.slice = Some((slice, of));
        COMMANDS.with_borrow_mut(Vec::clear);
        let menus = d.sweep_menus();
        for line in COMMANDS.take() {
            laps.line(&format!("{tag} {line}"));
        }
        let name = format!("menus slice {slice} of {of} ({menus} commands)");
        lap(&name, d);
        let tools = d.sweep_toolbars();
        let name = format!("toolbars slice {slice} of {of} ({tools} buttons)");
        lap(&name, d);
        d.slice = None;
    }
    assert!(
        d.app.project.model == imported_model,
        "browsing changed the project"
    );
    assert_eq!(d.app.history.counts(), (0, 0), "browsing added undo steps");
    if part != Part::Rest {
        let secs = started.elapsed().as_secs_f64();
        laps.line(&format!("{tag} done: {secs:.1}s, {part:?}"));
        return;
    }

    // F9, Level, Check Schedule.
    let scheduled = d.f9();
    lap("F9", d);
    d.level();
    lap("level", d);
    d.check_schedule();
    lap("check schedule", d);
    if scheduled {
        // A cancelled dialog after F9 still changes nothing.
        d.f9();
    }

    // Edits: grid name, grid duration, details steps, relationship drag,
    // assignment, copy and paste.
    d.home();
    let rows = d.visible_activities();
    assert!(!rows.is_empty(), "the table lists activities");
    let target = *rows
        .iter()
        .find(|&&i| {
            let a = &d.app.project.model.activities[i];
            // Expected finish recomputes remaining duration on F9.
            !a.is_completed() && a.task_type == TaskType::Task && a.expect_end.is_none()
        })
        .unwrap_or(&rows[0]);
    let task_id = d.app.project.model.activities[target].task_id;
    d.select(target);
    let pos = d.cell(target, "activity_name").unwrap();
    d.double_click_at(pos);
    assert_eq!(d.app.edit_cell, Some((target, "activity_name".to_string())));
    d.key_with(Key::A, ctrl());
    d.type_text("QA renamed activity");
    d.key(Key::Enter);
    assert_eq!(d.app.project.details[&task_id].name, "QA renamed activity");
    lap("grid edit name", d);
    let pos = d
        .cell(target, "remaining_duration")
        .expect("Remaining Duration is a default column");
    let before = d.app.project.model.activities[target].rem;
    d.double_click_at(pos);
    if d.app.edit_cell == Some((target, "remaining_duration".to_string())) {
        d.key_with(Key::A, ctrl());
        d.type_text("7d");
        d.key(Key::Enter);
        assert_ne!(
            d.app.project.model.activities[target].rem, before,
            "remaining duration edit applied: {:?}",
            d.app.error
        );
    } else {
        // Resource-driven durations refuse the editor with a reason.
        assert!(
            d.app.status.contains("resource units"),
            "the Remaining Duration editor opens or says why: {}",
            d.app.status
        );
        d.notes
            .push(format!("remaining duration not editable: {}", d.app.status));
    }
    lap("grid edit duration", d);
    // Details: a step.
    d.click("detail_tab_steps");
    if d.has("detail_steps_add") {
        d.click("detail_steps_add");
        lap("details step", d);
    }
    // Relationship by endpoint drag, with the whole project in view.
    d.home();
    d.click("fit");
    let links = d.app.project.model.links.len();
    let mut linked = false;
    let mut refused = vec![];
    for (p, s) in link_candidates(d).into_iter().take(6) {
        let bar = |id| {
            d.app
                .bar_geometry
                .iter()
                .find(|b| b.task_id == id)
                .map(|b| (b.link_grip(Edge::Start), b.link_grip(Edge::Finish)))
        };
        let (Some((_, from)), Some((to, _))) = (bar(p), bar(s)) else {
            continue;
        };
        let _ = d.frame(vec![egui::Event::PointerMoved(from)]);
        let _ = d.frame(vec![egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        for f in [0.25, 0.5, 0.75, 1.] {
            let _ = d.frame(vec![egui::Event::PointerMoved(from.lerp(to, f))]);
        }
        let dragging = d.app.link_drag.is_some();
        let _ = d.frame(vec![egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        d.frames(2);
        if d.has("sequence_yes") {
            d.click("sequence_yes");
        }
        d.assert_clean("relationship drag");
        if d.app.project.model.links.len() == links + 1 {
            linked = true;
            break;
        }
        refused.push(format!(
            "drag {dragging} {from:?}->{to:?}: {}",
            d.app.status
        ));
    }
    if linked {
        lap("relationship drag", d);
    } else {
        d.notes.push(format!(
            "no relationship drag landed ({} candidates): {refused:?}",
            link_candidates(d).len(),
        ));
    }
    // Assignment through Add Resource.
    d.home();
    let target = d
        .app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap();
    d.select(target);
    d.click("detail_tab_resources");
    let assignments = d.app.project.assignment_rows(task_id).len();
    let first = d.app.project.document.table("RSRC").and_then(|t| {
        t.rows
            .iter()
            .map(|r| t.value(r, "rsrc_short_name").to_string())
            .find(|name| !name.trim().is_empty())
    });
    if d.has("detail_rsrc_add") && first.is_some() {
        d.click("detail_rsrc_add");
        if let Some(name) = first {
            let found = d.click_text(&name);
            if found && d.has("rsrc_picker_assign") {
                d.click("rsrc_picker_assign");
            }
        }
        if d.has("rsrc_picker_close") {
            d.click("rsrc_picker_close");
        }
        if d.app.project.assignment_rows(task_id).len() == assignments + 1 {
            lap("assignment", d);
        } else {
            d.notes
                .push(format!("assignment not added: {}", d.app.status));
        }
    } else {
        d.notes.push("no resources to assign".into());
    }
    // Copy and paste the activity.
    d.home();
    d.select(target);
    let count = d.app.project.model.activities.len();
    // The platform turns Ctrl+C into a Copy event and Ctrl+V into a Paste
    // event carrying the system clipboard, which holds what Copy wrote.
    let output = d.frame_with(vec![egui::Event::Copy], ctrl());
    let copied = output
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .expect("Copy puts the rows on the clipboard");
    let _ = d.frame_with(vec![egui::Event::Paste(copied)], ctrl());
    if d.has("copy_options_ok") {
        d.click("copy_options_ok");
    }
    d.frames(2);
    assert_eq!(
        d.app.project.model.activities.len(),
        count + 1,
        "paste adds the copied activity: {}",
        d.app.status
    );
    lap("copy and paste", d);

    d.sweep_funding();
    lap("funding", d);
    d.sweep_batch3();
    lap("batch 3 surfaces", d);

    // Undo everything, then redo everything.
    let edited_model = d.app.project.model.clone();
    let edited_details = d.app.project.details.clone();
    let steps = d.undo_all();
    assert!(
        d.app.project.model == imported_model,
        "undo all restores the imported model\n{}",
        model_diff(&imported_model, &d.app.project.model)
    );
    assert!(
        d.app.project.details == imported_details,
        "undo all restores the imported details"
    );
    let redone = d.redo_all();
    assert_eq!(steps, redone);
    assert!(
        d.app.project.model == edited_model,
        "redo all\n{}",
        model_diff(&edited_model, &d.app.project.model)
    );
    assert!(d.app.project.details == edited_details, "redo all details");
    lap(&format!("undo/redo ({steps} steps)"), d);

    // F9 the edited network, save, reopen.
    let scheduled = d.f9();
    lap("F9 after edits", d);
    d.key_with(Key::S, ctrl());
    lap("ctrl+s", d);
    assert!(!d.app.dirty, "Ctrl+S saved");
    let reopened = d.app.store.last().unwrap().unwrap();
    // Loading rebuilds the assignment inputs from TASKRSRC, as F9 does first.
    let mut rebuilt = d.app.project.clone();
    rebuilt.refresh_derived_durations();
    let live = as_saved(&rebuilt.model);
    assert!(
        as_saved(&reopened.model) == live,
        "reopen: model\n{}",
        model_diff(&live, &as_saved(&reopened.model))
    );
    assert!(reopened.details == d.app.project.details, "reopen: details");
    lap("save and reopen", d);

    // Export XER and both spreadsheet layouts, read them back.
    let xer = d.dir.join("export.xer");
    d.export(interchange::ExportFormat::Xer, &xer);
    lap("export xer", d);
    let doc = Document::read(&xer).unwrap();
    let key = d.app.project.model.project.proj_id;
    let mut back = ProjectData::from_document(doc, key, "reimport".into()).unwrap();
    assert_eq!(
        back.model.activities.len(),
        d.app.project.model.activities.len()
    );
    assert_eq!(back.model.links.len(), d.app.project.model.links.len());
    if scheduled {
        assert_same_results(
            &d.app.project.model,
            &back.model,
            "stored dates after reimport",
        );
        back.refresh_derived_durations();
        schedule_model(&mut back.model, &back.resources).unwrap();
        assert_same_results(&d.app.project.model, &back.model, "F9 after reimport");
    }
    lap("reimport and F9", d);
    for (format, name) in [
        (interchange::ExportFormat::Modern, "modern.xlsx"),
        (interchange::ExportFormat::Tables, "tables.xlsx"),
    ] {
        let path = d.dir.join(name);
        d.export(format, &path);
        let (counts, samples) = d.spreadsheet_plan(&path);
        assert_eq!(
            counts.changed + counts.new + counts.delete + counts.errors,
            0,
            "{name}: reimporting an unchanged export changes nothing: {counts:?} {samples:?}"
        );
        assert_eq!(
            counts.links_new + counts.links_changed + counts.links_deleted,
            0,
            "{name}: relationships unchanged: {counts:?}"
        );
        lap(name, d);
    }
    for note in &d.notes {
        println!("  {label} note: {note}");
    }
    let secs = started.elapsed().as_secs_f64();
    laps.line(&format!("{tag} done: {secs:.1}s, {part:?}"));
}

/// The starter project, imported from its own export through File >
/// Import, as a user would.
fn starter_driver() -> Driver {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("starter", p);
    let file = d.dir.join("starter.xer");
    d.app.project.updated_document().write(&file).unwrap();
    d.import(&file);
    assert_eq!(d.app.project.model.activities.len(), 14);
    d
}

/// One part of the scenario on the starter project.
fn starter_part(test: &str, part: Part) {
    if part != Part::Rest && std::env::var_os("OPENXER_QA_NO_MENUS").is_some() {
        return;
    }
    let laps = Laps::new(test);
    let mut d = starter_driver();
    scenario(&mut d, "starter", part, &laps, "starter");
}

/// Everything on the starter project except its menu and toolbar
/// commands, which the `_menus_N` tests below try, one slice each.
#[test]
fn starter_project_survives_the_full_ui_sweep() {
    starter_part("starter_project_survives_the_full_ui_sweep", Part::Rest);
}

fn starter_slice(slice: usize) {
    let test = format!("starter_project_survives_the_full_ui_sweep_menus_{slice}");
    let of = MENU_SLICES;
    starter_part(&test, Part::Browse { slice, of });
}

#[test]
fn starter_project_survives_the_full_ui_sweep_menus_0() {
    starter_slice(0);
}

#[test]
fn starter_project_survives_the_full_ui_sweep_menus_1() {
    starter_slice(1);
}

#[test]
fn starter_project_survives_the_full_ui_sweep_menus_2() {
    starter_slice(2);
}

#[test]
fn starter_project_survives_the_full_ui_sweep_menus_3() {
    starter_slice(3);
}

#[test]
fn starter_project_survives_the_full_ui_sweep_menus_4() {
    starter_slice(4);
}

#[test]
fn starter_project_survives_the_full_ui_sweep_menus_5() {
    starter_slice(5);
}

/// One test per menu slice, in slice order: the array's length is
/// `MENU_SLICES`, so adding a slice without its test does not compile.
const STARTER_SLICE_TESTS: [fn(); MENU_SLICES] = [
    starter_project_survives_the_full_ui_sweep_menus_0,
    starter_project_survives_the_full_ui_sweep_menus_1,
    starter_project_survives_the_full_ui_sweep_menus_2,
    starter_project_survives_the_full_ui_sweep_menus_3,
    starter_project_survives_the_full_ui_sweep_menus_4,
    starter_project_survives_the_full_ui_sweep_menus_5,
];

fn corpus_dir() -> Option<PathBuf> {
    let dir = crate::test_corpus::dir();
    dir.is_dir().then_some(dir)
}

/// The corpus sweep is split over this many tests, which the test runner
/// runs side by side; each runs its share of the work units
/// ([`corpus_units`], [`corpus_shards`]).
const CORPUS_SHARDS: usize = 8;

/// A corpus file and its position in the sorted list of corpus files.
struct CorpusFile {
    index: usize,
    path: PathBuf,
    name: String,
    size: u64,
}

/// The corpus files in name order (only those `OPENXER_QA_ONLY` names,
/// when it is set). None when there is no corpus.
fn corpus_files() -> Option<Vec<CorpusFile>> {
    let dir = corpus_dir()?;
    let only = std::env::var("OPENXER_QA_ONLY").ok();
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "xer"))
        .collect();
    paths.sort();
    let mut files = vec![];
    for (index, path) in paths.into_iter().enumerate() {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if only
            .as_ref()
            .is_some_and(|o| !o.split(',').any(|x| x == name))
        {
            continue;
        }
        let size = std::fs::metadata(&path).map_or(0, |m| m.len());
        files.push(CorpusFile {
            index,
            path,
            name,
            size,
        });
    }
    Some(files)
}

/// Why the sweep leaves a corpus file out, or None when it sweeps it.
/// Debug builds skip files over 200 KB unless `OPENXER_QA_LARGE` is set.
fn corpus_skip(file: &CorpusFile) -> Option<String> {
    let large = !cfg!(debug_assertions) || std::env::var_os("OPENXER_QA_LARGE").is_some();
    let size = file.size;
    if size > 200_000 && !large {
        let why = format!("{size} bytes in a debug build (set OPENXER_QA_LARGE=1)");
        return Some(why);
    }
    let doc = match Document::read(&file.path) {
        Ok(doc) => doc,
        Err(e) => return Some(format!("not readable ({e})")),
    };
    let single = "not a single importable project";
    let projects = openxer_interchange::baseline::schedule_projects(&doc);
    if projects.len() != 1 {
        return Some(single.into());
    }
    let key = projects[0].0;
    let name = file.name.clone();
    if ProjectData::from_document(doc, key, name).is_err() {
        return Some(single.into());
    }
    None
}

/// How much work a unit is, from the lap times: a menu slice costs about
/// two and a half Rest parts.
fn unit_weight(part: Part) -> usize {
    match part {
        Part::Browse { .. } => 5,
        Part::Rest => 2,
    }
}

/// The files of `files` that get the full menu and toolbar sweep. A debug
/// build gives it to the first swept file only, because frames of large
/// projects cost tens of milliseconds unoptimized; a release build or
/// `OPENXER_QA_FULL=1` gives it to every file, and `OPENXER_QA_NO_MENUS=1`
/// to none.
fn corpus_full(files: &[CorpusFile]) -> Vec<usize> {
    if std::env::var_os("OPENXER_QA_NO_MENUS").is_some() {
        return vec![];
    }
    if !cfg!(debug_assertions) || std::env::var_os("OPENXER_QA_FULL").is_some() {
        return (0..files.len()).collect();
    }
    files
        .iter()
        .position(|f| corpus_skip(f).is_none())
        .into_iter()
        .collect()
}

/// The work units of the corpus sweep over `count` files, in a fixed order:
/// (file position, part). Every file has its Rest part; a file in `full`
/// also has one Browse part per menu slice.
fn corpus_units(count: usize, full: &[usize]) -> Vec<(usize, Part)> {
    let mut units = vec![];
    for i in 0..count {
        if full.contains(&i) {
            let of = MENU_SLICES;
            units.extend((0..of).map(|slice| (i, Part::Browse { slice, of })));
        }
        units.push((i, Part::Rest));
    }
    units
}

/// The test each unit goes to: the largest units first, each to the test
/// with the least work so far ([`unit_weight`]).
fn corpus_shards(units: &[(usize, Part)], shards: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|&u| (std::cmp::Reverse(unit_weight(units[u].1)), u));
    let mut load = vec![0; shards];
    let mut out = vec![0; units.len()];
    for u in order {
        let shard = (0..shards).min_by_key(|&s| (load[s], s)).unwrap();
        load[shard] += unit_weight(units[u].1);
        out[u] = shard;
    }
    out
}

/// Runs the corpus units [`corpus_shards`] gives to `shard`, each on a
/// fresh import of its file.
fn corpus_shard(shard: usize) {
    let Some(files) = corpus_files() else {
        println!("SKIPPED: XER corpus not found (set OPENXER_CORPUS)");
        return;
    };
    let test = format!("corpus_projects_survive_the_full_ui_sweep_{shard:02}");
    let laps = Laps::new(&test);
    let units = corpus_units(files.len(), &corpus_full(&files));
    let owner = corpus_shards(&units, CORPUS_SHARDS);
    let mut checked = 0;
    for (u, &(i, part)) in units.iter().enumerate() {
        if owner[u] != shard {
            continue;
        }
        let file = &files[i];
        if let Some(why) = corpus_skip(file) {
            println!("SKIPPED {}: {why}", file.name);
            continue;
        }
        println!("{} {part:?}:", file.name);
        let started = Instant::now();
        let mut d = Driver::new(&file.name, openxer_xer::starter());
        d.import(&file.path);
        let tag = format!("corpus #{} ({} KB)", file.index, file.size / 1024);
        let secs = started.elapsed().as_secs_f64();
        laps.line(&format!("{tag} imported: {secs:.1}s"));
        scenario(&mut d, &file.name, part, &laps, &tag);
        if part == Part::Rest {
            checked += 1;
        }
    }
    println!("checked {checked} corpus projects");
    laps.line(&format!("checked {checked} corpus projects"));
}

#[test]
fn corpus_projects_survive_the_full_ui_sweep_00() {
    corpus_shard(0);
}

#[test]
fn corpus_projects_survive_the_full_ui_sweep_01() {
    corpus_shard(1);
}

#[test]
fn corpus_projects_survive_the_full_ui_sweep_02() {
    corpus_shard(2);
}

#[test]
fn corpus_projects_survive_the_full_ui_sweep_03() {
    corpus_shard(3);
}

#[test]
fn corpus_projects_survive_the_full_ui_sweep_04() {
    corpus_shard(4);
}

#[test]
fn corpus_projects_survive_the_full_ui_sweep_05() {
    corpus_shard(5);
}

#[test]
fn corpus_projects_survive_the_full_ui_sweep_06() {
    corpus_shard(6);
}

#[test]
fn corpus_projects_survive_the_full_ui_sweep_07() {
    corpus_shard(7);
}

/// One test per corpus shard: the array's length is `CORPUS_SHARDS`, so a
/// shard without its test does not compile.
const CORPUS_SHARD_TESTS: [fn(); CORPUS_SHARDS] = [
    corpus_projects_survive_the_full_ui_sweep_00,
    corpus_projects_survive_the_full_ui_sweep_01,
    corpus_projects_survive_the_full_ui_sweep_02,
    corpus_projects_survive_the_full_ui_sweep_03,
    corpus_projects_survive_the_full_ui_sweep_04,
    corpus_projects_survive_the_full_ui_sweep_05,
    corpus_projects_survive_the_full_ui_sweep_06,
    corpus_projects_survive_the_full_ui_sweep_07,
];

/// The split of the sweeps drops nothing: every command key is in exactly
/// one menu slice, every corpus unit goes to one existing shard, and a
/// file with the full sweep has every slice.
#[test]
fn the_split_sweep_covers_every_command_and_project() {
    assert_eq!(STARTER_SLICE_TESTS.len(), MENU_SLICES);
    assert_eq!(CORPUS_SHARD_TESTS.len(), CORPUS_SHARDS);
    for key in [
        "file||menu_import",
        "view|Layout|menu_layout_open",
        "toolbar|tool_bars",
    ] {
        let slices = (0..MENU_SLICES)
            .filter(|&s| in_slice(key, Some((s, MENU_SLICES))))
            .count();
        assert_eq!(slices, 1, "{key}");
        assert!(in_slice(key, None));
    }
    // 15 files, the second with the full sweep.
    let units = corpus_units(15, &[1]);
    assert_eq!(units.len(), 15 + MENU_SLICES);
    for i in 0..15 {
        assert!(units.contains(&(i, Part::Rest)), "file {i}");
    }
    for slice in 0..MENU_SLICES {
        let of = MENU_SLICES;
        assert!(
            units.contains(&(1, Part::Browse { slice, of })),
            "slice {slice}"
        );
    }
    let owner = corpus_shards(&units, CORPUS_SHARDS);
    assert_eq!(owner.len(), units.len());
    assert!(owner.iter().all(|&s| s < CORPUS_SHARDS));
    // Every shard has work, so none of the tests sits idle.
    for s in 0..CORPUS_SHARDS {
        assert!(owner.contains(&s), "shard {s} has no work: {owner:?}");
    }
}

/// Regression: pressing on a table row while the pointer was moving
/// scrolled the table by the movement (mouse drag-to-scroll), so the press
/// landed on another row and the click selected nothing or the wrong
/// activity. The table scrolls by wheel and scroll bar only, as in the reference.
#[test]
fn pressing_a_row_while_the_pointer_moves_selects_it_without_scrolling() {
    let mut p = openxer_xer::starter();
    let wbs = p.wbs[0].id;
    for _ in 0..80 {
        p.add_activity(wbs);
    }
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("press-scroll", p);
    d.home();
    let rows = d.visible_activities();
    let target = rows[6];
    let pos = d.cell(target, "activity_id").unwrap();
    let before: Vec<_> = d.app.bar_geometry.iter().map(|b| b.row.top()).collect();
    // The pointer arrives from 120 px below in the same frame as the press.
    let _ = d.frame(vec![egui::Event::PointerMoved(pos + Vec2::new(0., 120.))]);
    for pressed in [true, false] {
        let _ = d.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]);
    }
    d.frames(1);
    assert_eq!(d.app.selected, Some(target), "the pressed row is selected");
    let after: Vec<_> = d.app.bar_geometry.iter().map(|b| b.row.top()).collect();
    assert_eq!(before, after, "a click does not scroll the table");
}

/// Regression: the WBS view opens on the Notebook tab, which laid out
/// the whole memo every frame although only the first lines fit; a 631 KB
/// memo in corpus 369 made opening the view take a 180 ms frame. Only the
/// text that can fill the viewer is laid out, and it starts the same way.
#[test]
fn wbs_notebook_lays_out_only_the_text_that_fits() {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let node = p.wbs[0].id;
    let line = "Scope narrative for the turnaround, line after line of text. ";
    let memo: String = line.repeat(10_000);
    crate::records::add_row(
        &mut p.document,
        "MEMOTYPE",
        &[("memo_type_id", "5".into()), ("memo_type", "Scope".into())],
    );
    crate::records::add_row(
        &mut p.document,
        "WBSMEMO",
        &[
            ("wbs_memo_id", "1".into()),
            ("proj_id", p.model.project.proj_id.to_string()),
            ("wbs_id", node.to_string()),
            ("memo_type_id", "5".into()),
            ("wbs_memo", memo.clone()),
        ],
    );
    let mut d = Driver::new("wbs-memo", p);
    d.click("directory_wbs");
    d.app.views.wbs.selected = Some(node);
    let texts = d.texts();
    let shown = texts
        .iter()
        .map(|(t, _)| t)
        .find(|t| t.starts_with("Scope narrative"))
        .expect("the memo is shown");
    assert!(
        shown.len() < 50_000,
        "laid out {} of {} characters",
        shown.len(),
        memo.len()
    );
    assert!(memo.starts_with(shown.as_str()));
}

/// The table's row build is reused between frames; every
/// input it depends on must still show up at once: project edits made
/// before or after the frame's undo snapshot, undo, search text, filters,
/// collapsed bands, grouping and display formats.
#[test]
fn reused_rows_follow_every_input() {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new("rows-cache", p);
    d.home();
    let ids = |d: &Driver| -> Vec<String> {
        d.app
            .rows()
            .into_iter()
            .map(|r| match r {
                Row::Activity(i) => d.app.project.model.activities[i].task_code.clone(),
                Row::Group(b) => format!("[{}]", d.app.band_label(b)),
            })
            .collect()
    };
    let start = ids(&d);
    assert_eq!(ids(&d), start, "an unchanged project gives the same rows");
    // A new activity shows before and after the frame records it.
    d.app.new_activity();
    let added = ids(&d);
    assert_eq!(added.len(), start.len() + 1);
    d.frames(2);
    assert_eq!(ids(&d), added);
    // An ID edit re-sorts.
    let i = d.app.project.model.activities.len() - 1;
    assert!(d.app.apply_cell(i, "activity_id", "AAA-FIRST"));
    assert!(ids(&d).contains(&"AAA-FIRST".to_string()));
    d.frames(2);
    // Undo removes the edit and then the activity.
    d.click("undo");
    assert!(!ids(&d).contains(&"AAA-FIRST".to_string()));
    d.click("undo");
    assert_eq!(ids(&d), start);
    // Search text, a filter, a collapsed band and no grouping.
    d.app.query = "RB-1010".into();
    assert_eq!(
        ids(&d).iter().filter(|r| !r.starts_with('[')).count(),
        1,
        "{:?}",
        ids(&d)
    );
    d.app.query.clear();
    d.app.grid.layout.filter.all_activities = false;
    d.app.grid.layout.filter.selected = vec![grid::filters::Standard::Milestone.key()];
    let milestones = d
        .app
        .project
        .model
        .activities
        .iter()
        .filter(|a| matches!(a.task_type, TaskType::Mile | TaskType::FinMile))
        .count();
    assert_eq!(
        ids(&d).iter().filter(|r| !r.starts_with('[')).count(),
        milestones
    );
    d.app.grid.layout.filter.all_activities = true;
    assert_eq!(ids(&d), start);
    let band = d.app.grid.bands.borrow()[0].key.clone();
    d.app.collapsed.insert(band.clone());
    assert!(ids(&d).len() < start.len());
    d.app.collapsed.remove(&band);
    d.app.grid.layout.group_sort.levels.clear();
    assert!(ids(&d).iter().all(|r| !r.starts_with('[')));
    d.frames(2);
    d.app.grid.layout.group_sort = Default::default();
    assert_eq!(ids(&d), start);
    // A detail table edit (records revision) with the model unchanged.
    d.app.touch_records();
    assert_eq!(ids(&d), start);
}
