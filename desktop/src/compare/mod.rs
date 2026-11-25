//! Tools > Schedule Comparison: pick an original project or baseline and a
//! revised project (workspace projects, their baselines, or a project in an
//! XER file), choose what to compare and where to send the report, then read
//! the result in an interactive window with filters and jump-to-activity, and
//! export it to HTML, Excel, PDF or CSV.
pub mod diff;
pub mod report;
#[cfg(test)]
mod tests;

use crate::dialog_kit::{self as kit, Entry};
use crate::reports::doc::{self, Output};
use crate::schedule_dialogs::modal;
use diff::{Area, Comparison, Kind2, Options};
use eframe::egui::{self, Align, Color32, Vec2};
use openxer_xer::{Document, ProjectData};
use report::ResultRow;
use std::path::PathBuf;

use super::App;

const PREF_KEY: &str = "schedule_comparison_v1";

/// Where one side of the comparison comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// The open project as it is now, saved or not.
    Open,
    /// A workspace project row.
    Workspace(i64),
    /// A stored baseline row.
    Baseline(i64),
    /// A project inside an XER file.
    File(PathBuf, i64),
}

pub struct Dialog {
    pub sources: Vec<(Source, String)>,
    pub old: usize,
    pub new: usize,
    pub options: Options,
    pub output: Output,
    pub save_file: bool,
    pub path: String,
    pub error: Option<String>,
}

pub struct Results {
    pub comparison: Comparison,
    pub rows: Vec<ResultRow>,
    /// Index into [`SHOW`].
    pub show: usize,
    /// 0 is every area, else `Area::ACTIVITY_FIELDS[area - 1]`.
    pub area: usize,
    pub find: String,
    pub selected: Option<usize>,
    pub note: Option<String>,
}

#[derive(Default)]
pub struct CompareState {
    pub dialog: Option<Dialog>,
    pub results: Option<Results>,
}

/// The Show filter of the results window.
pub const SHOW: [&str; 7] = [
    "All Changes",
    "Added",
    "Deleted",
    "Changed",
    "Activities",
    "Relationships",
    "Project and WBS",
];

fn show_matches(show: usize, r: &ResultRow) -> bool {
    match show {
        1 => r.kind == Kind2::Added,
        2 => r.kind == Kind2::Deleted,
        3 => r.kind == Kind2::Changed,
        4 => !matches!(r.area, Area::Relationships | Area::Project | Area::Wbs),
        5 => r.area == Area::Relationships,
        6 => matches!(r.area, Area::Project | Area::Wbs),
        _ => true,
    }
}

impl Results {
    pub fn new(comparison: Comparison) -> Self {
        let rows = report::rows(&comparison);
        Self {
            comparison,
            rows,
            show: 0,
            area: 0,
            find: String::new(),
            selected: None,
            note: None,
        }
    }
    /// Rows passing the filters, as indexes into `rows`.
    pub fn visible(&self) -> Vec<usize> {
        let find = self.find.trim().to_lowercase();
        let area = self
            .area
            .checked_sub(1)
            .and_then(|k| Area::ACTIVITY_FIELDS.get(k).copied());
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| show_matches(self.show, r))
            .filter(|(_, r)| area.is_none_or(|a| r.area == a))
            .filter(|(_, r)| {
                find.is_empty()
                    || r.code.to_lowercase().contains(&find)
                    || r.name.to_lowercase().contains(&find)
                    || r.field.to_lowercase().contains(&find)
            })
            .map(|(i, _)| i)
            .collect()
    }
}

fn options_json(d: &Dialog) -> String {
    serde_json::json!({
        "areas": d.options.areas.iter().map(|a| a.key()).collect::<Vec<_>>(),
        "group_by_activity": d.options.group_by_activity,
        "output": d.output.key(),
        "save_file": d.save_file,
        "path": d.path,
    })
    .to_string()
}

fn restore_options(d: &mut Dialog, text: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if let Some(list) = v["areas"].as_array() {
        d.options.areas = list
            .iter()
            .filter_map(|k| k.as_str().and_then(Area::from_key))
            .collect();
    }
    if let Some(b) = v["group_by_activity"].as_bool() {
        d.options.group_by_activity = b;
    }
    if let Some(o) = v["output"].as_str().and_then(Output::from_key) {
        d.output = o;
    }
    if let Some(b) = v["save_file"].as_bool() {
        d.save_file = b;
    }
    if let Some(p) = v["path"].as_str() {
        d.path = p.into();
    }
}

/// Read one project of an XER file.
pub fn load_file(path: &std::path::Path, pid: i64) -> Result<ProjectData, String> {
    let doc = Document::read(path)?;
    ProjectData::from_document(doc, pid, path.display().to_string())
}

impl App {
    /// Tools menu entry.
    pub(crate) fn compare_menu_item(&mut self, ui: &mut egui::Ui) {
        if self.item(
            ui,
            "menu_schedule_comparison",
            crate::chrome::Item::new("Schedule Comparison..."),
        ) {
            self.open_schedule_comparison();
        }
    }

    /// Every source the user can compare: the open project, each workspace
    /// project and each stored baseline.
    pub(crate) fn comparison_sources(&self) -> Vec<(Source, String)> {
        let open = format!(
            "<Current Project> {} - {}",
            self.project.model.project.name,
            crate::views::project_title(&self.project)
        );
        let mut out = vec![(Source::Open, open)];
        let projects = self.store.list().unwrap_or_default();
        for (id, title) in &projects {
            out.push((Source::Workspace(*id), format!("Project: {title} (saved)")));
        }
        let conn = self.store.connection();
        for (id, title) in &projects {
            for b in crate::baselines::store::list(conn, *id).unwrap_or_default() {
                out.push((
                    Source::Baseline(b.id),
                    format!("Baseline: {} ({title})", b.name),
                ));
            }
        }
        out
    }

    pub(crate) fn open_schedule_comparison(&mut self) {
        let sources = self.comparison_sources();
        // Original: the open project's own first baseline, else the first
        // other source; revised: the open project.
        let parent = self.workspace_row();
        let own_baseline = parent.and_then(|p| {
            let ids: Vec<i64> = crate::baselines::store::list(self.store.connection(), p)
                .unwrap_or_default()
                .into_iter()
                .map(|b| b.id)
                .collect();
            sources
                .iter()
                .position(|(s, _)| matches!(s, Source::Baseline(id) if ids.contains(id)))
        });
        let old = own_baseline.unwrap_or(if sources.len() > 1 { 1 } else { 0 });
        let mut d = Dialog {
            sources,
            old,
            new: 0,
            options: Options::default(),
            output: Output::Html,
            save_file: false,
            path: doc::documents()
                .join("ScheduleComparison.html")
                .display()
                .to_string(),
            error: None,
        };
        if let Ok(Some(text)) = self.store.preference(PREF_KEY) {
            restore_options(&mut d, &text);
        }
        self.compare.dialog = Some(d);
    }

    /// Load one side of the comparison.
    pub(crate) fn load_source(&self, source: &Source) -> Result<ProjectData, String> {
        match source {
            Source::Open => Ok(self.project.clone()),
            Source::Workspace(id) => self.store.load(*id),
            Source::Baseline(id) => {
                let row = crate::baselines::store::get(self.store.connection(), *id)?
                    .ok_or("The baseline no longer exists.")?;
                let mut document = Document::parse(&row.xer)?;
                document.windows_encoding = row.windows_encoding;
                ProjectData::from_document(document, row.project_key, row.name.clone())
            }
            Source::File(path, pid) => load_file(path, *pid),
        }
    }

    /// Run the comparison the dialog describes; on success the dialog closes
    /// and the results window opens (and the report file is written when
    /// asked for).
    pub(crate) fn run_schedule_comparison(&mut self) -> Result<(), String> {
        let d = self
            .compare
            .dialog
            .as_ref()
            .ok_or("No comparison is open.")?;
        let (old_source, old_label) = d
            .sources
            .get(d.old)
            .cloned()
            .ok_or("Pick an original project.")?;
        let (new_source, new_label) = d
            .sources
            .get(d.new)
            .cloned()
            .ok_or("Pick a revised project.")?;
        if old_source == new_source {
            return Err("Pick two different projects to compare.".into());
        }
        let options = d.options.clone();
        let output = d.output;
        let path = d.save_file.then(|| PathBuf::from(d.path.trim()));
        let _ = self.store.set_preference(PREF_KEY, &options_json(d));
        let old = self.load_source(&old_source)?;
        let new = self.load_source(&new_source)?;
        let comparison = diff::compare(&old, &old_label, &new, &new_label, &options);
        let mut results = Results::new(comparison);
        if let Some(path) = path.filter(|p| !p.as_os_str().is_empty()) {
            let r = report::report(&results.comparison);
            doc::write(&r, output, &path)?;
            results.note = Some(doc::saved_note(&path));
        }
        self.status = format!(
            "Schedule Comparison: {} activities added, {} deleted, {} changed",
            results.comparison.count(Kind2::Added),
            results.comparison.count(Kind2::Deleted),
            results.comparison.count(Kind2::Changed)
        );
        self.compare.results = Some(results);
        self.compare.dialog = None;
        Ok(())
    }

    /// Write the current results as a report file.
    pub(crate) fn export_comparison(
        &mut self,
        format: Output,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let results = self
            .compare
            .results
            .as_ref()
            .ok_or("No comparison results.")?;
        let r = report::report(&results.comparison);
        doc::write(&r, format, path)?;
        if let Some(results) = self.compare.results.as_mut() {
            results.note = Some(doc::saved_note(path));
        }
        Ok(())
    }

    /// Select the activity with `code` in the open project.
    pub(crate) fn jump_to_code(&mut self, code: &str) -> bool {
        match self
            .project
            .model
            .activities
            .iter()
            .position(|a| a.task_code == code)
        {
            Some(i) => {
                self.views.go(crate::views::MainView::Activities);
                self.jump_to(i);
                true
            }
            None => {
                self.status = format!("Activity {code} is not in the open project.");
                false
            }
        }
    }

    pub(crate) fn compare_dialogs(&mut self, ctx: &egui::Context) {
        if self.compare.dialog.is_some() {
            self.compare_dialog(ctx);
        }
        self.compare_results_window(ctx);
    }

    fn compare_dialog(&mut self, ctx: &egui::Context) {
        #[derive(Default)]
        struct Clicks {
            compare: bool,
            cancel: bool,
            help: bool,
            browse_old: bool,
            browse_new: bool,
            browse_out: bool,
        }
        let Some(mut d) = self.compare.dialog.take() else {
            return;
        };
        const W: f32 = 640.;
        const H: f32 = 404.;
        let labels: Vec<String> = d.sources.iter().map(|s| s.1.clone()).collect();
        let items: Vec<(&str, bool)> = labels.iter().map(|l| (l.as_str(), true)).collect();
        let (k, closed, actions) = modal(
            ctx,
            "compare",
            "Schedule Comparison",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                c.lowered(5., 5., W - 122., H - 10.);
                c.label(16., 18., 17., "Original Project or Baseline", true);
                c.combo(
                    "compare_old",
                    196.,
                    16.,
                    286.,
                    22.,
                    &items,
                    &mut d.old,
                    true,
                );
                k.browse_old =
                    kit::button(c, "compare_browse_old", 486., 16., 26., 22., "...", true);
                c.label(16., 48., 17., "Revised Project", true);
                c.combo(
                    "compare_new",
                    196.,
                    46.,
                    286.,
                    22.,
                    &items,
                    &mut d.new,
                    true,
                );
                k.browse_new =
                    kit::button(c, "compare_browse_new", 486., 46., 26., 22., "...", true);
                kit::section(c, 12., 80., W - 136., 214., "Select information to compare");
                let left = [
                    Area::Project,
                    Area::Wbs,
                    Area::Activities,
                    Area::Relationships,
                ];
                for (n, area) in left.iter().enumerate() {
                    let mut on = d.options.has(*area);
                    c.checkbox(
                        &format!("compare_area_{}", area.key()),
                        22.,
                        110. + 22. * n as f32,
                        230.,
                        17.,
                        area.caption(),
                        &mut on,
                        true,
                    );
                    set(&mut d.options, *area, on);
                }
                kit::text(c, 270., 106., "Activity Information", true);
                for (n, area) in Area::ACTIVITY_FIELDS.iter().enumerate() {
                    let mut on = d.options.has(*area);
                    c.checkbox(
                        &format!("compare_area_{}", area.key()),
                        280.,
                        126. + 20. * n as f32,
                        220.,
                        17.,
                        area.caption(),
                        &mut on,
                        true,
                    );
                    set(&mut d.options, *area, on);
                }
                c.checkbox(
                    "compare_group_by_activity",
                    22.,
                    204.,
                    230.,
                    17.,
                    "Group report by activity",
                    &mut d.options.group_by_activity,
                    true,
                );
                kit::section(c, 12., 302., W - 136., 92., "Send Report To");
                for (n, o) in Output::ALL.iter().enumerate() {
                    if c.radio(
                        &format!("compare_output_{}", o.key()),
                        22. + 118. * n as f32,
                        330.,
                        110.,
                        17.,
                        o.caption(),
                        d.output == *o,
                        true,
                    ) && d.output != *o
                    {
                        d.output = *o;
                        d.path = PathBuf::from(&d.path)
                            .with_extension(o.extension())
                            .display()
                            .to_string();
                    }
                }
                c.checkbox(
                    "compare_save_file",
                    22.,
                    360.,
                    92.,
                    17.,
                    "Output File",
                    &mut d.save_file,
                    true,
                );
                c.edit(
                    "compare_path",
                    118.,
                    358.,
                    360.,
                    22.,
                    &mut d.path,
                    d.save_file,
                    Align::Min,
                );
                let frame = c.r(118., 358., 381., 22.);
                if c.ellipsis("compare_path_browse", frame, d.save_file)
                    .clicked()
                {
                    k.browse_out = true;
                }
                if let Some(e) = &d.error {
                    kit::error_text(c, 16., 67., W - 140., e);
                }
                kit::strip(c, W, H);
                let column = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("compare_run", "Compare", true, false),
                        Entry::cancel("compare_cancel"),
                        Entry::help("compare_help"),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                k.compare = clicked("compare_run");
                k.cancel = clicked("compare_cancel");
                k.help = clicked("compare_help");
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.browse_old || k.browse_new {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("XER", &["xer"])
                .pick_file()
            {
                match Document::read(&path) {
                    Ok(doc) => {
                        let projects = openxer_interchange::baseline::schedule_projects(&doc);
                        if projects.is_empty() {
                            d.error = Some("No projects found in this XER.".into());
                        }
                        let first = d.sources.len();
                        for (pid, name) in projects {
                            let file = path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            d.sources.push((
                                Source::File(path.clone(), pid),
                                format!("File: {name} ({file})"),
                            ));
                        }
                        if d.sources.len() > first {
                            if k.browse_old {
                                d.old = first;
                            } else {
                                d.new = first;
                            }
                        }
                    }
                    Err(e) => d.error = Some(e),
                }
            }
        }
        if k.browse_out {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(d.output.caption(), &[d.output.extension()])
                .set_file_name(format!("ScheduleComparison.{}", d.output.extension()))
                .save_file()
            {
                d.path = path.display().to_string();
            }
        }
        if k.help {
            self.open_help_for("compare");
        }
        if k.cancel || closed {
            return;
        }
        self.compare.dialog = Some(d);
        if k.compare {
            if let Err(e) = self.run_schedule_comparison() {
                if let Some(d) = self.compare.dialog.as_mut() {
                    d.error = Some(e);
                }
            }
        }
    }

    fn compare_results_window(&mut self, ctx: &egui::Context) {
        let Some(mut r) = self.compare.results.take() else {
            return;
        };
        const W: f32 = 960.;
        const H: f32 = 560.;
        #[derive(Default)]
        struct Clicks {
            close: bool,
            go: bool,
            export: Option<Output>,
            help: bool,
            jump: Option<usize>,
        }
        let visible = r.visible();
        let (k, closed, actions) = kit::window(
            ctx,
            "compare_results",
            "Schedule Comparison Results",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                let cmp = &r.comparison;
                kit::text(
                    c,
                    10.,
                    8.,
                    &format!("Comparing {} to {}", cmp.old_label, cmp.new_label),
                    true,
                );
                let finish = match cmp.finish_variance {
                    Some(v) => format!(
                        "Project finish {} to {} ({})",
                        cmp.finish.0.text(),
                        cmp.finish.1.text(),
                        diff::signed_days(v)
                    ),
                    None => "Project finish not available".into(),
                };
                kit::text(
                    c,
                    10.,
                    26.,
                    &format!(
                        "{finish}.   Activities: {} added, {} deleted, {} changed.   Relationships: {} added, {} deleted, {} changed.",
                        cmp.count(Kind2::Added),
                        cmp.count(Kind2::Deleted),
                        cmp.count(Kind2::Changed),
                        cmp.link_count(Kind2::Added),
                        cmp.link_count(Kind2::Deleted),
                        cmp.link_count(Kind2::Changed)
                    ),
                    true,
                );
                c.label(10., 50., 17., "Show", true);
                let show: Vec<(&str, bool)> = SHOW.iter().map(|s| (*s, true)).collect();
                c.combo(
                    "compare_show",
                    46.,
                    48.,
                    170.,
                    22.,
                    &show,
                    &mut r.show,
                    true,
                );
                c.label(234., 50., 17., "Area", true);
                let mut areas: Vec<(&str, bool)> = vec![("All Areas", true)];
                areas.extend(Area::ACTIVITY_FIELDS.iter().map(|a| (a.short(), true)));
                c.combo(
                    "compare_area",
                    268.,
                    48.,
                    170.,
                    22.,
                    &areas,
                    &mut r.area,
                    true,
                );
                c.label(456., 50., 17., "Find", true);
                c.edit(
                    "compare_find",
                    488.,
                    48.,
                    230.,
                    22.,
                    &mut r.find,
                    true,
                    Align::Min,
                );
                let cols = [
                    kit::Col {
                        title: "Change",
                        width: 64.,
                    },
                    kit::Col {
                        title: "Activity ID",
                        width: 104.,
                    },
                    kit::Col {
                        title: "Activity Name",
                        width: 210.,
                    },
                    kit::Col {
                        title: "Field",
                        width: 140.,
                    },
                    kit::Col {
                        title: "Original",
                        width: 120.,
                    },
                    kit::Col {
                        title: "Revised",
                        width: 120.,
                    },
                    kit::Col {
                        title: "Variance",
                        width: 74.,
                    },
                ];
                let selected = r
                    .selected
                    .and_then(|s| visible.iter().position(|&v| v == s));
                let rows = &r.rows;
                let bottom = c.origin.y + H - 7.;
                let out = kit::list(
                    c,
                    "compare_rows",
                    (6., 78., W - 124., H - 84.),
                    None,
                    &cols,
                    visible.len(),
                    selected,
                    None,
                    &mut |c, i, col, rect, sel| {
                        let row = &rows[visible[i]];
                        let s = match col {
                            0 => row.kind.caption(),
                            1 => row.code.as_str(),
                            2 => row.name.as_str(),
                            3 => row.field.as_str(),
                            4 => row.old.as_str(),
                            5 => row.new.as_str(),
                            _ => row.variance.as_str(),
                        };
                        if rect.bottom() > bottom {
                            return;
                        }
                        if col == 0 && !sel {
                            let tint = match row.kind {
                                Kind2::Added => Color32::from_rgb(0xDF, 0xF5, 0xDF),
                                Kind2::Deleted => Color32::from_rgb(0xF9, 0xDA, 0xDA),
                                Kind2::Changed => Color32::from_rgb(0xFF, 0xF7, 0xCC),
                            };
                            c.ui.painter()
                                .rect_filled(rect.shrink2(Vec2::new(1., 1.)), 0., tint);
                        }
                        crate::reports::cell_text(c, rect, s, sel, 3., bottom);
                    },
                );
                if let Some(i) = out.clicked {
                    r.selected = Some(visible[i]);
                }
                if let Some(i) = out.double {
                    k.jump = Some(visible[i]);
                }
                if let Some(note) = &r.note {
                    kit::text(c, 730., 52., note, true);
                }
                kit::strip(c, W, H);
                let keys: Vec<String> = Output::ALL
                    .iter()
                    .map(|o| format!("compare_export_{}", o.key()))
                    .collect();
                let mut entries = vec![
                    Entry::close("compare_results_close"),
                    Entry::Button(
                        "compare_results_goto",
                        "Go To",
                        r.selected.is_some_and(|s| r.rows[s].jump.is_some()),
                        true,
                    ),
                ];
                for (n, (o, key)) in Output::ALL.iter().zip(&keys).enumerate() {
                    let label = match o {
                        Output::Html => "Export HTML...",
                        Output::Excel => "Export Excel...",
                        Output::Pdf => "Export PDF...",
                        Output::Csv => "Export CSV...",
                    };
                    entries.push(Entry::Button(key, label, true, n == 0));
                }
                entries.push(Entry::help("compare_results_help"));
                let column = kit::button_column(c, W, &entries);
                let clicked = column.clicked.as_deref();
                k.close = clicked == Some("compare_results_close");
                k.go = clicked == Some("compare_results_goto");
                k.export = Output::ALL
                    .iter()
                    .zip(&keys)
                    .find(|(_, key)| clicked == Some(key.as_str()))
                    .map(|(o, _)| *o);
                k.help = clicked == Some("compare_results_help");
                k
            },
        );
        self.record_dialog_actions(actions);
        let jump = k
            .jump
            .or(if k.go { r.selected } else { None })
            .and_then(|i| r.rows[i].jump.clone());
        self.compare.results = Some(r);
        if let Some(code) = jump {
            self.jump_to_code(&code);
        }
        if let Some(format) = k.export {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(format.caption(), &[format.extension()])
                .set_file_name(format!("ScheduleComparison.{}", format.extension()))
                .save_file()
            {
                if let Err(e) = self.export_comparison(format, &path) {
                    self.error(e);
                }
            }
        }
        if k.help {
            self.open_help_for("compare_results");
        }
        if k.close || closed {
            self.compare.results = None;
        }
    }
}

fn set(options: &mut Options, area: Area, on: bool) {
    if on {
        options.areas.insert(area);
    } else {
        options.areas.remove(&area);
    }
}
