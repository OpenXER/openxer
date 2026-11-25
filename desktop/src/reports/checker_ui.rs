//! Tools > Project Checker: pick projects, choose the checks and their
//! limits, generate the report (saved as HTML, Excel, PDF or CSV by the file
//! extension) and read the findings in a results window with jump-to.
use super::checker::{self, Config, GroupBy, ProjectResult, Rule, Severity};
use super::doc::{self, Output};
use crate::dialog_kit as kit;
use crate::schedule_dialogs::modal;
use crate::App;
use eframe::egui::{self, Align, Color32, Key, Pos2, Vec2};
use std::path::PathBuf;

const PREF_KEY: &str = "project_checker_v1";

/// A project the checker can run on.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Open,
    Workspace(i64),
}

pub struct CheckerDialog {
    pub selected: Vec<(Target, String)>,
    pub available: Vec<(Target, String)>,
    pub selected_row: Option<usize>,
    pub available_row: Option<usize>,
    pub config: Config,
    pub rule_row: Option<usize>,
    pub limits: [String; 3],
    pub path: String,
    pub overwrite: bool,
    pub error: Option<String>,
}

pub struct CheckerResults {
    pub projects: Vec<ProjectResult>,
    /// Which project result is the open project (findings jump into it).
    pub open: Option<usize>,
    pub config: Config,
    /// Index into `rows()`.
    pub selected: usize,
    pub finding: Option<usize>,
    pub note: Option<String>,
}

impl CheckerResults {
    /// (project, rule result) pairs in display order.
    pub fn rows(&self) -> Vec<(usize, usize)> {
        self.projects
            .iter()
            .enumerate()
            .flat_map(|(p, pr)| (0..pr.results.len()).map(move |r| (p, r)))
            .filter(|&(p, r)| !self.config.errors_only || !self.projects[p].results[r].passed())
            .collect()
    }

    /// The line above the results: every check that ran, and how many
    /// found problems, whether or not Report Errors Only hides the passed
    /// ones from the list.
    pub fn summary(&self) -> String {
        let results = self.projects.iter().flat_map(|p| p.results.iter());
        let run = results.clone().count();
        let found = results.filter(|r| !r.passed()).count();
        let mut text = format!("{run} checks run; {found} found problems.");
        if self.config.errors_only && found < run {
            text.push_str(" Passed checks are not listed.");
        }
        text
    }
}

fn number(v: f64) -> String {
    crate::records::format_number(v)
}

impl App {
    pub(crate) fn project_checker_menu_item(&mut self, ui: &mut egui::Ui) {
        if self.item(
            ui,
            "menu_project_checker",
            crate::chrome::Item::new("Project Checker...").icon(crate::chrome::Icon::CheckSchedule),
        ) {
            self.open_project_checker();
        }
    }

    pub(crate) fn open_project_checker(&mut self) {
        let open_label = format!(
            "{} - {}",
            self.project.model.project.name,
            crate::views::project_title(&self.project)
        );
        let own = self.workspace_row();
        let available = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, _)| Some(*id) != own)
            .map(|(id, title)| (Target::Workspace(id), title))
            .collect();
        let config = self
            .store
            .preference(PREF_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .map(|v| Config::from_json(&v))
            .unwrap_or_default();
        let path = self
            .store
            .preference("project_checker_path_v1")
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                doc::documents()
                    .join("ProjectCheckerReport.html")
                    .display()
                    .to_string()
            });
        self.reports.checker = Some(CheckerDialog {
            selected: vec![(Target::Open, open_label)],
            available,
            selected_row: None,
            available_row: None,
            limits: [
                number(config.long_lag_days),
                number(config.long_duration_days),
                number(config.large_float_days),
            ],
            config,
            rule_row: None,
            path,
            overwrite: true,
            error: None,
        });
    }

    /// Check the selected projects; on success the results window opens and
    /// the report is written when a path is given.
    pub(crate) fn run_project_checker(&mut self) -> Result<(), String> {
        let d = self
            .reports
            .checker
            .as_mut()
            .ok_or("The checker is not open.")?;
        for (i, text) in d.limits.clone().iter().enumerate() {
            let v: f64 = text
                .trim()
                .parse()
                .ok()
                .filter(|v: &f64| v.is_finite() && *v >= 0.)
                .ok_or_else(|| format!("{text:?} is not a number of days."))?;
            match i {
                0 => d.config.long_lag_days = v,
                1 => d.config.long_duration_days = v,
                _ => d.config.large_float_days = v,
            }
        }
        if d.selected.is_empty() {
            return Err("Add at least one project to the list.".into());
        }
        let config = d.config.clone();
        let targets = d.selected.clone();
        let path = d.path.trim().to_string();
        let overwrite = d.overwrite;
        let _ = self
            .store
            .set_preference(PREF_KEY, &config.to_json().to_string());
        let _ = self.store.set_preference("project_checker_path_v1", &path);
        let mut projects = vec![];
        let mut open = None;
        for (target, label) in &targets {
            let results = match target {
                Target::Open => {
                    open = Some(projects.len());
                    checker::run(&self.project, &config)
                }
                Target::Workspace(id) => checker::run(&self.store.load(*id)?, &config),
            };
            projects.push(ProjectResult {
                label: label.clone(),
                results,
            });
        }
        let mut note = None;
        if !path.is_empty() {
            let path = PathBuf::from(&path);
            let format =
                Output::from_path(&path).ok_or("Save the report as .html, .xlsx, .pdf or .csv.")?;
            if path.exists() && !overwrite {
                return Err(format!("{} already exists.", path.display()));
            }
            doc::write(&checker::report(&projects, &config), format, &path)?;
            note = Some(doc::saved_note(&path));
        }
        let found: usize = projects
            .iter()
            .flat_map(|p| &p.results)
            .filter(|r| !r.passed())
            .count();
        self.status = format!("Project Checker: {found} checks found problems");
        self.reports.checker = None;
        self.reports.checker_results = Some(CheckerResults {
            projects,
            open,
            config,
            selected: 0,
            finding: None,
            note,
        });
        Ok(())
    }

    pub(crate) fn export_checker(
        &mut self,
        format: Output,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let r = self
            .reports
            .checker_results
            .as_ref()
            .ok_or("No checker results.")?;
        doc::write(&checker::report(&r.projects, &r.config), format, path)?;
        if let Some(r) = self.reports.checker_results.as_mut() {
            r.note = Some(doc::saved_note(path));
        }
        Ok(())
    }

    pub(crate) fn checker_dialogs(&mut self, ctx: &egui::Context) {
        if self.reports.checker.is_some() {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.reports.checker = None;
            } else {
                self.checker_dialog(ctx);
            }
        }
        self.checker_results_window(ctx);
    }

    fn checker_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.checker.take() else {
            return;
        };
        const W: f32 = 780.;
        const H: f32 = 540.;
        #[derive(Default)]
        struct Clicks {
            run: bool,
            close: bool,
            help: bool,
            browse: bool,
        }
        let (k, closed, actions) = modal(ctx, "checker", "Project Checker", Vec2::new(W, H), |c| {
            let mut k = Clicks::default();
            c.fill(0., 0., W, H, kit::FACE);
            // Projects.
            kit::section(c, 5., 5., W - 122., 150., "Select Projects");
            let sel = &d.selected;
            let out = kit::list(
                c,
                "checker_selected",
                (12., 32., 270., 116.),
                None,
                &[kit::Col {
                    title: "Selected Projects",
                    width: 266.,
                }],
                sel.len(),
                d.selected_row,
                None,
                &mut |c, i, _, rect, s| kit::cell_text(c, rect, &sel[i].1, s, 3.),
            );
            if let Some(i) = out.clicked {
                d.selected_row = Some(i);
            }
            let add = kit::button(
                c,
                "checker_add",
                292.,
                58.,
                160.,
                25.,
                "< Add to list",
                d.available_row.is_some(),
            );
            let remove = kit::button(
                c,
                "checker_remove",
                292.,
                92.,
                160.,
                25.,
                "Remove from list >",
                d.selected_row.is_some(),
            );
            let av = &d.available;
            let out = kit::list(
                c,
                "checker_available",
                (462., 32., 190., 116.),
                None,
                &[kit::Col {
                    title: "Available Projects",
                    width: 186.,
                }],
                av.len(),
                d.available_row,
                None,
                &mut |c, i, _, rect, s| kit::cell_text(c, rect, &av[i].1, s, 3.),
            );
            if let Some(i) = out.clicked {
                d.available_row = Some(i);
            }
            if add {
                if let Some(i) = d.available_row.take() {
                    let item = d.available.remove(i);
                    d.selected.push(item);
                }
            }
            if remove {
                if let Some(i) = d.selected_row.take() {
                    let item = d.selected.remove(i);
                    d.available.push(item);
                }
            }
            // Checks.
            kit::section(c, 5., 162., W - 122., 262., "Options");
            for (n, g) in checker::Group::ALL.iter().enumerate() {
                let rules: Vec<Rule> = Rule::ALL
                    .iter()
                    .copied()
                    .filter(|r| r.group() == *g)
                    .collect();
                let mut all = rules.iter().all(|r| d.config.rules.contains(r));
                let before = all;
                c.checkbox(
                    &format!("checker_group_{n}"),
                    14.,
                    190. + 22. * n as f32,
                    170.,
                    17.,
                    g.caption(),
                    &mut all,
                    true,
                );
                if all != before {
                    for r in rules {
                        if all {
                            d.config.rules.insert(r);
                        } else {
                            d.config.rules.remove(&r);
                        }
                    }
                }
            }
            c.checkbox(
                "checker_priority",
                14.,
                262.,
                180.,
                17.,
                "Run Priority Checks Only",
                &mut d.config.priority_only,
                true,
            );
            c.checkbox(
                "checker_errors_only",
                14.,
                284.,
                180.,
                17.,
                "Report Errors Only",
                &mut d.config.errors_only,
                true,
            );
            kit::text(c, 14., 312., "Group By Options", true);
            for (n, (g, label)) in [(GroupBy::Check, "Check"), (GroupBy::Activity, "Activity")]
                .into_iter()
                .enumerate()
            {
                if c.radio(
                    &format!("checker_group_by_{n}"),
                    24. + 86. * n as f32,
                    330.,
                    80.,
                    17.,
                    label,
                    d.config.group_by == g,
                    true,
                ) {
                    d.config.group_by = g;
                }
            }
            for (n, label) in [
                "Lag limit (days)",
                "Duration limit (days)",
                "Float limit (days)",
            ]
            .iter()
            .enumerate()
            {
                let y = 356. + 22. * n as f32;
                c.label(14., y + 2., 17., label, true);
                c.edit(
                    &format!("checker_limit_{n}"),
                    136.,
                    y,
                    56.,
                    20.,
                    &mut d.limits[n],
                    true,
                    Align::Max,
                );
            }
            let config = &d.config;
            let bottom = c.origin.y + 188. + 230. - 1.;
            let out = kit::list(
                c,
                "checker_rules",
                (206., 188., W - 334., 230.),
                None,
                &[
                    kit::Col {
                        title: "Check",
                        width: 300.,
                    },
                    kit::Col {
                        title: "Severity",
                        width: 70.,
                    },
                    kit::Col {
                        title: "Priority",
                        width: 56.,
                    },
                ],
                Rule::ALL.len(),
                d.rule_row,
                None,
                &mut |c, i, col, rect, s| {
                    let rule = Rule::ALL[i];
                    let enabled = config.runs(rule);
                    if rect.bottom() > bottom {
                        return;
                    }
                    match col {
                        0 => {
                            let local =
                                Pos2::new(rect.left() + 3., rect.top() + 2.) - c.origin.to_vec2();
                            kit::check_glyph(
                                c,
                                local.x,
                                local.y,
                                config.rules.contains(&rule),
                                true,
                            );
                            let caption = if enabled {
                                rule.caption().to_string()
                            } else {
                                format!("{} (off)", rule.caption())
                            };
                            kit::cell_text(c, rect, &caption, s, 20.);
                        }
                        1 => kit::cell_text(
                            c,
                            rect,
                            match rule.severity() {
                                Severity::Error => "Error",
                                Severity::Warning => "Warning",
                            },
                            s,
                            3.,
                        ),
                        _ => kit::cell_text(
                            c,
                            rect,
                            if rule.priority() { "High" } else { "" },
                            s,
                            3.,
                        ),
                    }
                },
            );
            if let Some(i) = out.clicked {
                let rule = Rule::ALL[i];
                // A click on the check box toggles; elsewhere selects.
                let pointer = c.ui.ctx().pointer_interact_pos().unwrap_or_default();
                let list_left = c.origin.x + 207.;
                if pointer.x < list_left + 20. && !d.config.rules.remove(&rule) {
                    d.config.rules.insert(rule);
                }
                d.rule_row = Some(i);
            }
            // Report file.
            kit::section(c, 5., 430., W - 122., 104., "Report");
            c.label(14., 462., 17., "Save Report To", true);
            c.edit(
                "checker_path",
                120.,
                460.,
                500.,
                22.,
                &mut d.path,
                true,
                Align::Min,
            );
            let frame = c.r(120., 460., 521., 22.);
            if c.ellipsis("checker_path_browse", frame, true).clicked() {
                k.browse = true;
            }
            c.checkbox(
                "checker_overwrite",
                14.,
                490.,
                160.,
                17.,
                "Overwrite Existing",
                &mut d.overwrite,
                true,
            );
            kit::text(
                c,
                190.,
                492.,
                "The file type follows the extension: .html, .xlsx, .pdf or .csv.",
                true,
            );
            if let Some(e) = &d.error {
                kit::error_text(c, 14., 509., W - 140., e);
            }
            kit::strip(c, W, H);
            let col = kit::button_column(
                c,
                W,
                &[
                    kit::Entry::Button(
                        "checker_run",
                        "Generate Report",
                        !d.selected.is_empty(),
                        false,
                    ),
                    kit::Entry::close("checker_close"),
                    kit::Entry::help("checker_help"),
                ],
            );
            match col.clicked.as_deref() {
                Some("checker_run") => k.run = true,
                Some("checker_close") => k.close = true,
                Some("checker_help") => k.help = true,
                _ => {}
            }
            k
        });
        self.record_dialog_actions(actions);
        if k.browse {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Report", &["html", "xlsx", "pdf", "csv"])
                .set_file_name("ProjectCheckerReport.html")
                .save_file()
            {
                d.path = path.display().to_string();
            }
        }
        if k.help {
            self.open_help_for("checker");
        }
        if k.close || closed {
            return;
        }
        self.reports.checker = Some(d);
        if k.run {
            if let Err(e) = self.run_project_checker() {
                if let Some(d) = self.reports.checker.as_mut() {
                    d.error = Some(e);
                }
            }
        }
    }

    fn checker_results_window(&mut self, ctx: &egui::Context) {
        let Some(mut r) = self.reports.checker_results.take() else {
            return;
        };
        const W: f32 = 900.;
        const H: f32 = 540.;
        #[derive(Default)]
        struct Clicks {
            close: bool,
            go: bool,
            jump: Option<usize>,
            export: Option<Output>,
            help: bool,
        }
        let rows = r.rows();
        if r.selected >= rows.len() {
            r.selected = 0;
        }
        let (k, closed, actions) = kit::window(
            ctx,
            "checker_results",
            "Project Checker Results",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                let many = r.projects.len() > 1;
                let projects = &r.projects;
                let out = kit::list(
                    c,
                    "checker_result_rows",
                    (6., 26., W - 124., 220.),
                    None,
                    &[
                        kit::Col {
                            title: "Check",
                            width: 360.,
                        },
                        kit::Col {
                            title: "Severity",
                            width: 80.,
                        },
                        kit::Col {
                            title: "Checked",
                            width: 70.,
                        },
                        kit::Col {
                            title: "Found",
                            width: 60.,
                        },
                        kit::Col {
                            title: "Result",
                            width: 70.,
                        },
                        kit::Col {
                            title: "Project",
                            width: 130.,
                        },
                    ],
                    rows.len(),
                    Some(r.selected),
                    None,
                    &mut |c, i, col, rect, s| {
                        let (p, k) = rows[i];
                        let res = &projects[p].results[k];
                        let text = match col {
                            0 => res.rule.caption().to_string(),
                            1 => match res.rule.severity() {
                                Severity::Error => "Error".into(),
                                Severity::Warning => "Warning".into(),
                            },
                            2 => res.checked.to_string(),
                            3 => res.findings.len().to_string(),
                            4 => {
                                if res.passed() {
                                    "Passed".into()
                                } else {
                                    "Found".into()
                                }
                            }
                            _ => {
                                if many {
                                    projects[p].label.clone()
                                } else {
                                    String::new()
                                }
                            }
                        };
                        if col == 4 && !s {
                            let tint = if res.passed() {
                                Color32::from_rgb(0xDF, 0xF5, 0xDF)
                            } else if res.rule.severity() == Severity::Error {
                                Color32::from_rgb(0xF9, 0xDA, 0xDA)
                            } else {
                                Color32::from_rgb(0xFF, 0xF7, 0xCC)
                            };
                            c.ui.painter().rect_filled(rect.shrink(1.), 0., tint);
                        }
                        kit::cell_text(c, rect, &text, s, 3.);
                    },
                );
                if let Some(i) = out.clicked {
                    r.selected = i;
                    r.finding = None;
                }
                kit::text(c, 8., 8., &r.summary(), true);
                let current = rows
                    .get(r.selected)
                    .map(|&(p, k)| (p, &projects[p].results[k]));
                if let Some((_, res)) = current {
                    kit::text(c, 8., 254., res.rule.advice(), true);
                }
                let findings = current.map_or(&[][..], |(_, res)| &res.findings[..]);
                let out = kit::list(
                    c,
                    "checker_findings",
                    (6., 274., W - 124., H - 280.),
                    None,
                    &[
                        kit::Col {
                            title: "Activity ID",
                            width: 120.,
                        },
                        kit::Col {
                            title: "Activity Name",
                            width: 280.,
                        },
                        kit::Col {
                            title: "Detail",
                            width: 370.,
                        },
                    ],
                    findings.len(),
                    r.finding,
                    None,
                    &mut |c, i, col, rect, s| {
                        let f = &findings[i];
                        let text = match col {
                            0 => &f.code,
                            1 => &f.name,
                            _ => &f.detail,
                        };
                        kit::cell_text(c, rect, text, s, 3.);
                    },
                );
                if let Some(i) = out.clicked {
                    r.finding = Some(i);
                }
                if let Some(i) = out.double {
                    k.jump = Some(i);
                }
                if let Some(note) = &r.note {
                    kit::text(c, 360., 8., note, true);
                }
                let can_jump =
                    current.is_some_and(|(p, _)| Some(p) == r.open) && r.finding.is_some();
                kit::strip(c, W, H);
                let exports: Vec<(String, &str, Output)> = Output::ALL
                    .iter()
                    .map(|o| {
                        let label = match o {
                            Output::Html => "Export HTML...",
                            Output::Excel => "Export Excel...",
                            Output::Pdf => "Export PDF...",
                            Output::Csv => "Export CSV...",
                        };
                        (format!("checker_export_{}", o.key()), label, *o)
                    })
                    .collect();
                let mut entries = vec![
                    kit::Entry::close("checker_results_close"),
                    kit::Entry::Button("checker_results_goto", "Go To", can_jump, true),
                ];
                for (n, (key, label, _)) in exports.iter().enumerate() {
                    entries.push(kit::Entry::Button(key, label, true, n == 0));
                }
                entries.push(kit::Entry::help("checker_results_help"));
                let col = kit::button_column(c, W, &entries);
                match col.clicked.as_deref() {
                    Some("checker_results_close") => k.close = true,
                    Some("checker_results_goto") => k.go = true,
                    Some("checker_results_help") => k.help = true,
                    Some(key) => {
                        k.export = exports.iter().find(|e| e.0 == key).map(|e| e.2);
                    }
                    None => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        let current = rows.get(r.selected).copied();
        let jump = k
            .jump
            .or(if k.go { r.finding } else { None })
            .and_then(|f| {
                let (p, res) = current?;
                (Some(p) == r.open).then(|| {
                    r.projects[p].results[res]
                        .findings
                        .get(f)
                        .map(|f| f.code.clone())
                })?
            });
        self.reports.checker_results = Some(r);
        if let Some(code) = jump.filter(|c| !c.is_empty()) {
            self.jump_to_code(&code);
        }
        if let Some(format) = k.export {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(format.caption(), &[format.extension()])
                .set_file_name(format!("ProjectCheckerReport.{}", format.extension()))
                .save_file()
            {
                if let Err(e) = self.export_checker(format, &path) {
                    self.error(e);
                }
            }
        }
        if k.help {
            self.open_help_for("checker_results");
        }
        if k.close || closed {
            self.reports.checker_results = None;
        }
    }
}
