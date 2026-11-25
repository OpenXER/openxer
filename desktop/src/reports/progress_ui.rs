//! S-Curve and Earned Hours, from Tools > Reports or from their rows in the
//! Reports view: one small dialog each with the defaults already filled in,
//! then the report in a report tab (chart and tables), where Export > Excel
//! writes the workbook, Refresh runs it again and Options... reopens the
//! dialog.
//!
//! The figures and the workbook are the command line's and MCP's
//! (`openxer_xer::scurve`, `openxer_interchange::scurve_xlsx`), so the app,
//! `openxer-cli export-scurve` and `export-earned-hours` write the same
//! numbers. Nothing here changes the project.
use super::progress_view::{self, Figures, ProgressDoc, Spec};
use super::report_tab::{Doc, ReportTabId, Source as TabSource};
use super::rpt_entries::{Builtin, Which};
use crate::baselines::data::{self, Slot};
use crate::dialog_kit::{self as kit, Entry};
use crate::App;
use eframe::egui::{self, Pos2, Rect, Vec2};
use openxer_interchange::scurve_xlsx::{self, Style};
use openxer_xer::scurve::{self, Breakdown, Detection, Group, Measure, Options, Period, Source};
use std::collections::BTreeSet;

/// The dialogs' names, as the dialog kit notes them (help topics key on
/// them).
pub const SCURVE_SALT: &str = "scurve";
pub const EARNED_SALT: &str = "earned_hours";

/// The S-Curve dialog: what was found and the choices, indexes into the
/// lists the detection offers.
pub struct ScurveDialog {
    pub detection: Detection,
    pub measure: usize,
    pub period: usize,
    pub breakdown: usize,
    pub late: bool,
    /// Only the activities the current filter shows.
    pub filtered: bool,
    /// The report tab whose options these are (Options... on the tab):
    /// OK runs the report again in it.
    pub tab: Option<ReportTabId>,
}

impl ScurveDialog {
    /// The defaults for what was found: its measure, weekly periods and
    /// its breakdown.
    pub fn new(detection: Detection) -> Self {
        let defaults = Options::detected(&detection);
        let mut d = Self {
            measure: 0,
            period: detection
                .periods()
                .iter()
                .position(|p| *p == Period::Week)
                .unwrap_or(0),
            breakdown: 0,
            late: false,
            filtered: false,
            tab: None,
            detection,
        };
        d.breakdown = d.breakdown_index(defaults.breakdown);
        d
    }

    pub fn measure(&self) -> Measure {
        let list = self.detection.measures();
        list[self.measure.min(list.len() - 1)]
    }

    fn breakdown_index(&self, b: Breakdown) -> usize {
        self.detection
            .breakdowns(self.measure())
            .iter()
            .position(|x| *x == b)
            .unwrap_or(0)
    }

    /// The options chosen, weeks starting on `sunday` or Monday.
    pub fn options(&self, sunday: bool) -> Options {
        let periods = self.detection.periods();
        let breakdowns = self.detection.breakdowns(self.measure());
        Options {
            measure: self.measure(),
            period: periods[self.period.min(periods.len() - 1)],
            breakdown: breakdowns[self.breakdown.min(breakdowns.len() - 1)],
            late: self.late,
            week_starts_sunday: sunday,
        }
    }

    /// A new detection (the scope changed): the same choices where they are
    /// still offered, else the defaults.
    fn redetect(&mut self, detection: Detection) {
        let before = self.options(true);
        let (filtered, tab) = (self.filtered, self.tab);
        *self = Self::new(detection);
        self.filtered = filtered;
        self.tab = tab;
        self.choose(before);
    }

    /// The choices of `before` where they are offered.
    fn choose(&mut self, before: Options) {
        self.late = before.late;
        if let Some(m) = self
            .detection
            .measures()
            .iter()
            .position(|m| *m == before.measure)
        {
            self.measure = m;
            self.breakdown = self.breakdown_index(before.breakdown);
        }
        if let Some(p) = self
            .detection
            .periods()
            .iter()
            .position(|p| *p == before.period)
        {
            self.period = p;
        }
    }
}

/// The Earned Hours dialog: the rows of its first sheet.
pub struct EarnedDialog {
    pub group: usize,
    /// The report tab whose options these are, as for the S-curve.
    pub tab: Option<ReportTabId>,
}

impl App {
    /// The open projects as the exports read them: the active project with
    /// its assigned baselines (only the activities its current filter shows
    /// when `filtered`), and without a filter the other open projects too.
    pub(crate) fn with_report_sources<R>(
        &self,
        filtered: bool,
        f: impl FnOnce(&[Source]) -> R,
    ) -> R {
        let only = filtered.then(|| self.filtered_activities());
        let compare = self.baseline_compare();
        let slot = |s: Slot| match compare.source(s) {
            data::Source::Baseline(l) => Some(l.clone()),
            _ => None,
        };
        let mut sources = vec![Source {
            project: &self.project,
            project_baseline: slot(Slot::Project),
            primary_baseline: slot(Slot::Primary),
            basis: compare.basis,
            only: only.as_ref(),
        }];
        if !filtered {
            sources.extend(self.open_set.parked.iter().map(|x| Source::new(&x.project)));
        }
        f(&sources)
    }

    /// The active project's activities the current filter shows, collapsed
    /// bands or not (all of them when no filter is on).
    pub(crate) fn filtered_activities(&self) -> BTreeSet<usize> {
        let n = self.project.model.activities.len();
        let layout = &self.grid.layout;
        if layout.filter.all_activities && self.query.trim().is_empty() && self.wbs_filter.is_none()
        {
            return (0..n).collect();
        }
        let ix = self.shared_index();
        let catalog = crate::grid::columns::catalog(self.shown());
        let none = BTreeSet::new();
        let built = crate::grid::group::build(
            self.shown(),
            &ix,
            &catalog,
            layout,
            &crate::grid::group::Scope {
                query: &self.query,
                wbs_filter: self.wbs_filter,
                collapsed: &none,
                users: &self.grid.user_filters,
            },
        );
        let rows: BTreeSet<usize> = if layout.filter.highlight && !layout.filter.all_activities {
            built.highlighted
        } else {
            built
                .rows
                .iter()
                .filter_map(|r| match r {
                    crate::grid::Row::Activity(i) => Some(*i),
                    _ => None,
                })
                .collect()
        };
        rows.into_iter().filter(|i| *i < n).collect()
    }

    /// How dates and costs show in the workbooks: the user's date format and
    /// currency.
    fn report_style(&self) -> Style {
        let c = &self.formats.currency;
        let symbol = if c.show_symbol { c.symbol.as_str() } else { "" };
        let decimals = if c.show_decimals { c.decimals } else { 0 };
        Style {
            date_format: self.formats.dates.excel_pattern(),
            currency_format: Some(scurve_xlsx::excel_currency(symbol, decimals.into())),
        }
    }

    /// Tools > Reports > S-Curve..., or Run on its row in the Reports view.
    pub(crate) fn open_scurve(&mut self) {
        let detection = self.with_report_sources(false, Detection::of);
        self.reports.scurve = Some(ScurveDialog::new(detection));
    }

    /// Tools > Reports > Earned Hours..., or Run on its row.
    pub(crate) fn open_earned_hours(&mut self) {
        self.reports.earned_hours = Some(EarnedDialog {
            group: 0,
            tab: None,
        });
    }

    /// Run on a built-in report's row: its dialog.
    pub(crate) fn open_builtin(&mut self, b: Builtin) {
        match b {
            Builtin::Scurve => self.open_scurve(),
            Builtin::EarnedHours => self.open_earned_hours(),
        }
    }

    /// Options... on a built-in report's tab: its dialog with the tab's
    /// choices; OK runs it again in that tab.
    pub(crate) fn open_progress_options(&mut self, id: ReportTabId) {
        let Some(TabSource::Progress(spec)) = self.reports.tabs.get(id).map(|t| &t.source) else {
            return;
        };
        match *spec {
            Spec::Scurve { options, filtered } => {
                let detection = self.with_report_sources(filtered, Detection::of);
                let mut d = ScurveDialog::new(detection);
                d.filtered = filtered;
                d.tab = Some(id);
                d.choose(options);
                self.reports.scurve = Some(d);
            }
            Spec::Earned { group } => {
                self.reports.earned_hours = Some(EarnedDialog {
                    group: Group::ALL.iter().position(|g| *g == group).unwrap_or(0),
                    tab: Some(id),
                });
            }
        }
    }

    /// The figures of `spec` on the open projects, with their tables and
    /// (for the S-curve) the chart drawn on the print canvas.
    pub(crate) fn progress_doc(&mut self, spec: Spec) -> Result<ProgressDoc, String> {
        let style = self.report_style();
        match spec {
            Spec::Scurve { options, filtered } => {
                let mut curve =
                    self.with_report_sources(filtered, |s| scurve::curve(s, options))?;
                if !filtered && !self.open_set.parked.is_empty() {
                    curve.warnings.push(
                        "The other open projects' budget is their current plan; their assigned baselines are not read here.".into(),
                    );
                }
                let scope = if filtered {
                    "Activities the current filter shows in the active project"
                } else {
                    "Every activity of the open projects"
                };
                let extra = vec![("Scope".to_string(), scope.to_string())];
                let report = progress_view::curve_report(&curve, &extra);
                let size = Vec2::new(super::pdf::figure_width(), progress_view::CHART_HEIGHT);
                let labels = self.label_options();
                let ctx = self.print_context();
                let ops = crate::print::compose::capture(&ctx, size, |ui| {
                    progress_view::paint_curve_chart(
                        ui,
                        Rect::from_min_size(Pos2::ZERO, size),
                        &curve,
                        &labels,
                    );
                });
                Ok(ProgressDoc {
                    report,
                    chart: Some((ops, size)),
                    figures: Figures::Curve(Box::new(curve)),
                    extra,
                    style,
                })
            }
            Spec::Earned { group } => {
                let e = self.with_report_sources(false, |s| scurve::earned_hours(&s[0], group))?;
                Ok(ProgressDoc {
                    report: progress_view::earned_report(&e),
                    chart: None,
                    figures: Figures::Earned(Box::new(e)),
                    extra: vec![],
                    style,
                })
            }
        }
    }

    /// OK in a dialog: the report in a new tab, or again in the tab whose
    /// Options... opened the dialog. A project without labor hours gets a
    /// message and no tab.
    pub(crate) fn show_progress(&mut self, spec: Spec, tab: Option<ReportTabId>) {
        let doc = match self.progress_doc(spec) {
            Ok(doc) => doc,
            Err(e) => {
                self.error(e);
                return;
            }
        };
        self.mark_run(&Which::Builtin(spec.builtin()));
        if let Some(mut t) = tab.and_then(|id| self.reports.tabs.take(id)) {
            t.source = TabSource::Progress(spec);
            t.set_doc(Doc::Progress(Box::new(doc)));
            t.note = Some("Options changed".into());
            t.saved = None;
            let id = t.id;
            self.reports.tabs.put(t);
            self.show_view(crate::views::MainView::Report(id));
            return;
        }
        self.open_report_tab(
            spec.builtin().name().into(),
            TabSource::Progress(spec),
            "",
            Doc::Progress(Box::new(doc)),
        );
    }

    pub(crate) fn scurve_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.scurve.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 560.;
        const H: f32 = 240.;
        let measures: Vec<&str> = d.detection.measures().iter().map(|m| m.label()).collect();
        let periods: Vec<&str> = d.detection.periods().iter().map(|p| p.label()).collect();
        let breakdowns: Vec<&str> = d
            .detection
            .breakdowns(d.measure())
            .iter()
            .map(|b| b.label())
            .collect();
        let items = |list: &[&'static str]| -> Vec<(&'static str, bool)> {
            list.iter().map(|s| (*s, true)).collect()
        };
        let (measures, periods, breakdowns) =
            (items(&measures), items(&periods), items(&breakdowns));
        let measure_before = d.measure;
        let filtered_before = d.filtered;
        let ((ok, cancel, help), closed, actions) =
            kit::window(ctx, SCURVE_SALT, "S-Curve", Vec2::new(W, H), |c| {
                c.fill(0., 0., W, H, kit::FACE);
                kit::text(c, 14., 14., "Measure", true);
                c.combo(
                    "scurve_measure",
                    14.,
                    30.,
                    260.,
                    21.,
                    &measures,
                    &mut d.measure,
                    true,
                );
                kit::secondary_text(c, 14., 56., &d.detection.line(d.measure()));
                kit::text(c, 14., 84., "Period", true);
                c.combo(
                    "scurve_period",
                    14.,
                    100.,
                    260.,
                    21.,
                    &periods,
                    &mut d.period,
                    true,
                );
                kit::text(c, 14., 130., "Break down by", true);
                c.combo(
                    "scurve_breakdown",
                    14.,
                    146.,
                    260.,
                    21.,
                    &breakdowns,
                    &mut d.breakdown,
                    true,
                );
                c.checkbox(
                    "scurve_late",
                    14.,
                    180.,
                    400.,
                    20.,
                    "Add the late dates curve",
                    &mut d.late,
                    true,
                );
                c.checkbox(
                    "scurve_filtered",
                    14.,
                    206.,
                    400.,
                    20.,
                    "Only activities in the current filter",
                    &mut d.filtered,
                    true,
                );
                kit::strip(c, W, H);
                let clicked = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("scurve_ok"),
                        Entry::cancel("scurve_cancel"),
                        Entry::help("scurve_help"),
                    ],
                )
                .clicked;
                let is = |key: &str| clicked.as_deref() == Some(key);
                (is("scurve_ok"), is("scurve_cancel"), is("scurve_help"))
            });
        self.record_dialog_actions(actions);
        if cancel || closed {
            return;
        }
        if help {
            self.open_help_for("scurve");
        }
        if d.measure != measure_before {
            let m = d.measure();
            d.breakdown = d.breakdown_index(d.detection.breakdown(m));
        }
        if d.filtered != filtered_before {
            let detection = self.with_report_sources(d.filtered, Detection::of);
            d.redetect(detection);
        }
        if ok {
            let sunday = self.label_options().week_starts_sunday;
            let spec = Spec::Scurve {
                options: d.options(sunday),
                filtered: d.filtered,
            };
            self.show_progress(spec, d.tab);
            return;
        }
        self.reports.scurve = Some(d);
    }

    pub(crate) fn earned_hours_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.earned_hours.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 420.;
        const H: f32 = 108.;
        let groups: Vec<(&str, bool)> = Group::ALL.iter().map(|g| (g.label(), true)).collect();
        let ((ok, cancel, help), closed, actions) =
            kit::window(ctx, EARNED_SALT, "Earned Hours", Vec2::new(W, H), |c| {
                c.fill(0., 0., W, H, kit::FACE);
                kit::text(c, 14., 14., "Break down by", true);
                c.combo(
                    "earned_hours_group",
                    14.,
                    30.,
                    200.,
                    21.,
                    &groups,
                    &mut d.group,
                    true,
                );
                kit::secondary_text(
                    c,
                    14.,
                    62.,
                    "Labor hours earned to the data date, with a sheet by activity.",
                );
                kit::strip(c, W, H);
                let clicked = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("earned_hours_ok"),
                        Entry::cancel("earned_hours_cancel"),
                        Entry::help("earned_hours_help"),
                    ],
                )
                .clicked;
                let is = |key: &str| clicked.as_deref() == Some(key);
                (
                    is("earned_hours_ok"),
                    is("earned_hours_cancel"),
                    is("earned_hours_help"),
                )
            });
        self.record_dialog_actions(actions);
        if cancel || closed {
            return;
        }
        if help {
            self.open_help_for("earned_hours");
        }
        if ok {
            let group = Group::ALL[d.group.min(Group::ALL.len() - 1)];
            self.show_progress(Spec::Earned { group }, d.tab);
            return;
        }
        self.reports.earned_hours = Some(d);
    }

    /// The file name Save As offers for a built-in report's workbook: the
    /// project, the report and the data date (so two updates' files sort
    /// side by side).
    pub(crate) fn report_file_name(&self, report: &str) -> String {
        let m = &self.project.model;
        let day = m
            .project
            .data_date
            .map(|d| openxer_xer::date_string(m.origin_serial, Some(d))[..10].to_string())
            .unwrap_or_default();
        let name = format!("{} {report} {day}", m.project.name);
        format!("{}.xlsx", crate::interchange::safe_file_name(name.trim()))
    }
}
